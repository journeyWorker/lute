---
title: Build a town game
description: Build a small, clock-driven town with Lute 0.46 and player-web.
---

This tutorial builds **Quayside**, a tiny Stardew-style town. The complete project is
[`docs/examples/quayside/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/quayside).
The snippets below are the files in that project, so the documentation check compiles
them as you read.

## 1. Schema and clock

Declare the state domains first. `run.slot` is a named `slot` domain, not an
unrelated string, and `run.ending` is a closed domain that lets the player name
an ending. The terminal expression ends the run as soon as a choice writes an
ending other than `none`.

```yaml check="docs/examples/quayside/world.schema.yaml"
enums:
  slot: [morning, afternoon, evening]
  ending: [none, restore, retire]
state:
  run.day: { type: int, default: 1, owner: engine }
  run.slot: { type: { domain: slot }, default: morning, owner: engine }
  run.ending: { type: { domain: ending }, default: none }
  run.visits: { type: int, default: 0 }
clock:
  day: run.day
  slot: run.slot
  slots: [morning, afternoon, evening]
  raise: { slot: slotStart, dayEnd: dayEnd }
  raiseAtStart: true
  week:
    length: 7
    first: 0
    labels: [Monday, Tuesday, Wednesday, Thursday, Friday, Saturday, Sunday]
entities:
  place: { members: [square, pier, bakery] }
relations: {}
defs:
  currentSlot: "run.slot"
terminal: "run.ending != 'none'"
cast:
  player: { name: You }
  june: { name: June }
  tom: { name: Tom }
```

`clock.index` remains numeric for ordering. For labels in a host HUD, use
`program.view(state).clock` rather than converting the numeric slot yourself;
the view supplies the slot and weekday labels.

## 2. Scenes, beats, and barks

The backdrop component has one arm per slot. Directive fields are build-time
values, so pass the live slot into the component and keep each `::bg` literal.
Use this component in every scene that follows the clock:

```lute check="docs/examples/quayside/components/clock-backdrop.component.lute"
---
component: clockBackdrop
params:
  slot: { type: { domain: slot } }
---
## Backdrop
<match subject="@slot">
  <when is="morning">
    ::bg{location="quayside" time="morning"}
  </when>
  <when is="afternoon">
    ::bg{location="quayside" time="afternoon"}
  </when>
  <when is="evening">
    ::bg{location="quayside" time="evening"}
  </when>
</match>
```

The `start` beat introduces the town:

```lute check="docs/examples/quayside/scenes/opening.lute"
---
kind: scene
id: opening
components: [../components/clock-backdrop.component.lute]
on: start
priority: 10
---

## Morning at Quayside

::use{component="clockBackdrop" slot=@currentSlot instance="opening-backdrop"}
@june: The tide is out, and Quayside is already awake.
@narrator: A hand-painted sign points toward the square, the pier, and the bakery.
```

A visit beat can offer a choice and write the ending:

```lute check="docs/examples/quayside/scenes/market.lute"
---
kind: scene
id: market
components: [../components/clock-backdrop.component.lute]
on: visit
target: place.square
priority: 10
once: false
---

## The market square

::use{component="clockBackdrop" slot=@currentSlot instance="market-backdrop"}
@june: Every stall has something to mend, trade, or remember.
::set{run.visits += 1}

<branch id="closingBell" prompt="The harbor bell rings. What will you do with the old town hall?">
  <choice id="restore" text="Restore it as a community hall">
    ::set{run.ending = 'restore'}
    @june: We will fill it with lanterns and neighbors.
  </choice>
  <choice id="wait" text="Wait for the bell to ring again">
    @june: We can take one more evening before deciding.
  </choice>
  <choice id="retire" text="Let the building rest with the tide">
    ::set{run.ending = 'retire'}
    @june: Some doors close gently, and that is a kind of care.
  </choice>
</branch>

<match subject="run.ending">
  <when is="restore">
    @narrator: The square keeps its lights on for the hall.
  </when>
  <when is="retire">
    @narrator: The square grows quiet around the old hall.
  </when>
  <when is="none">
    @narrator: The bell waits for your answer.
  </when>
</match>
```

The `tide` beat is gated until evening:


```lute check="docs/examples/quayside/scenes/tide.lute"
---
kind: scene
id: tide
components: [../components/clock-backdrop.component.lute]
on: tide
---

## Evening tide

::use{component="clockBackdrop" slot=@currentSlot instance="tide-backdrop"}
@narrator: At evening tide, the town lights come on one by one.
```

A lore document supplies static background barks. They do not write state or
consume a presentation when a custom host displays them while a player waits:

```lute check="docs/examples/quayside/lore/barks.lute"
---
kind: lore
id: barks
title: Quayside background voices
---

<entry id="marketBark" category="bark" on="bark" priority="-1">
  @june: Fresh bread, repaired nets, and one more story before sunset.
</entry>

<entry id="pierBark" category="bark" on="visit" target="place.pier" priority="-1">
  @tom: Mind the loose board. The pier remembers every storm.
</entry>
```

## 3. Plays, refusals, and coverage

Play both named endings. The first script deliberately raises `tide` in the
morning. Its refusal is expected, the state remains unchanged, and the play
continues to the market choice:

```yaml check="docs/examples/quayside/plays/restore.play.yaml"
# Rebuild the old town hall and keep Quayside's doors open.
steps:
  - occasion: start
    expect: { winner: opening }
  - occasion: tide
    expect: { refused: E-OCCASION-GATE, state: { run.ending: none } }
  - occasion: visit
    target: place.square
    choose: { closingBell: restore }
    expect: { winner: market, state: { run.ending: restore } }
expect:
  end: terminal
  state: { run.ending: restore }
```

```yaml check="docs/examples/quayside/plays/retire.play.yaml"
# Let the old town hall rest with the tide.
steps:
  - occasion: start
    expect: { winner: opening }
  - occasion: visit
    target: place.square
    choose: { closingBell: retire }
    expect: { winner: market, state: { run.ending: retire } }
expect:
  end: terminal
  state: { run.ending: retire }
```

The coverage play also walks afternoon and evening, opens `tide`, and presents
both static barks:

```yaml check="docs/examples/quayside/plays/coverage.play.yaml"
# Exercise every backdrop slot, the gated occasion, the static barks, and ending none.
steps:
  - occasion: start
    expect: { winner: opening }
  - occasion: bark
    expect: { winner: marketBark }
  - occasion: visit
    target: place.pier
    expect: { winner: pierBark }
  - occasion: visit
    target: place.square
    choose: { closingBell: wait }
    expect: { winner: market, state: { run.ending: none } }
  - advance: { to: afternoon }
  - occasion: visit
    target: place.square
    choose: { closingBell: wait }
    expect: { winner: market, state: { run.ending: none } }
  - advance: { to: evening }
  - occasion: tide
    expect: { winner: tide }
  - occasion: visit
    target: place.square
    choose: { closingBell: retire }
    expect: { winner: market, state: { run.ending: retire } }
expect:
  end: terminal
  state: { run.ending: retire }
```

Coverage now reports the market backdrop match `3/3`, ending match `3/3`, all
three branch choices, and both bark entries. The opening and tide components
remain `1/3` by design: those scenes are only entered at run start (morning)
and by the evening gate respectively. A gated raise is a play assertion, not
a workaround in the host.

## 4. Bindings and named ending screens

Bindings resolve paths below `assetsBase`. `ui.title.music` uses the `assets:`
key `title-music`; `ui.ended` reads `run.ending` and selects a screen for each
named ending:

```yaml check="docs/examples/quayside/player.bindings.yaml"
bindingsVersion: "0.46"
canvas: { width: 1280, height: 720 }
assetsBase: assets/
assets:
  title-music: audio/title.wav
backgrounds:
  quayside:
    image: bg/quayside-morning.png
    times:
      morning: bg/quayside-morning.png
      afternoon: bg/quayside-afternoon.png
      evening: bg/quayside-evening.png
characters:
  june:
    sprite: { default: char/june.png }
    nameColor: "#f2b56b"
  tom:
    sprite: { default: char/tom.png }
    nameColor: "#78b5e8"
ui:
  font: "Georgia, serif"
  title: { background: bg/title.png, music: title-music }
  ended:
    by: run.ending
    screens:
      restore: { title: "The Hall Opens", text: "Quayside gathers under one roof.", background: bg/restore.png }
      retire: { title: "A Gentle Closing", text: "The town lets the old hall rest.", background: bg/retire.png }
```

Every referenced placeholder file is committed under `docs/examples/quayside/assets/`.
The manifest uses `bindingsVersion: "0.46"`; see the [bindings reference](/player/bindings/)
for the complete catalogue.

## 5. A player-web host

Keep the walkable town map in the host's own Pixi application. `player-web`
owns its player assets privately; it does not initialise Pixi's global `Assets`.
That means the host may load its map before or after mounting:

```ts
import { Assets, Application, Sprite } from "pixi.js"
import { mountPlayer } from "@lute-lang/player-web"

const app = new Application()
await app.init({ resizeTo: window })
const map = Sprite.from(await Assets.load("/town-map.png"))
app.stage.addChild(map) // The host map, outside the Lute player.

// `Assets` belongs only to this map app; player-web has a private loader.
const options = {
  bindings: { url: "/quayside/player.bindings.yaml" },
  bundleUrl: "/quayside/bundle/",
  wasmUrl: "/wasm/lute_runtime_wasm_bg.wasm"
} as const

// The Promise facade keeps the mounted scope alive until unmount.
const player = await mountPlayer(document.querySelector("#player")!, options)
```

Keep that `player` for the page's lifetime and call `player.unmount()` when the
page goes away. Its `program` queries take the session's current state
(`player.session.current`). To open a gated occasion, query first and then send
only when its gate is open:

```ts
import { Effect, SubscriptionRef } from "effect"

const current = await Effect.runPromise(SubscriptionRef.get(player.session.current))
const gate = await Effect.runPromise(player.program.gate(current.state, "tide"))
if (gate.open) await player.send({ type: "raiseOccasion", occasion: "tide" })
```

While a choice or bridge is waiting, a host can show one of Quayside's static
barks without starting a second presentation. Ask `program.candidates`, find an
eligible `entry`, and read its placeholder-free lines from `player.bundle`.
The complete artifact-walking helper is in [custom hosts](/player/custom-hosts/);
the essential 0.45 pattern is:

```ts
declare const showAmbientFromBundle: (bundle: unknown, candidate: unknown) => void
const current = await Effect.runPromise(SubscriptionRef.get(player.session.current))
const candidates = await Effect.runPromise(
  player.program.candidates(current.state, "visit", "place.pier")
)
const bark = candidates.find((candidate) => candidate.kind === "entry")
if (bark !== undefined) {
  showAmbientFromBundle(player.bundle, bark) // static lines; no read or once mark
}
```

A bark that has placeholders, a `match`, a `once`, or a write must wait until
idle and be raised with `send` instead.

## 6. Save, resume, and the clock HUD

The Promise facade exposes the same save operations as the Effect handle. Start
a fresh mount from `save.snapshot`, then restore the full player save:

```ts
import { mountPlayer } from "@lute-lang/player-web"

const player = await mountPlayer(document.querySelector("#player")!, options)
const save = await player.snapshot()
await player.unmount()

const resumed = await mountPlayer(document.querySelector("#player")!, {
  ...options,
  snapshot: save.snapshot
})
await resumed.restore(save)
```

The Effect API uses the same option. `mount` needs a `Scope`, and the player
lives exactly as long as that scope, so run this with the page's long-lived
scope (not `Effect.scoped`, which would close it as soon as the effect returns):

```ts
import { Effect } from "effect"
import { mount } from "@lute-lang/player-web"

// Effect<void, unknown, Scope.Scope>: provide the page's scope when you run it.
const resumeInPageScope = Effect.gen(function* () {
  const resumed = yield* mount(document.querySelector("#player")!, {
    ...options,
    snapshot: save.snapshot
  })
  yield* resumed.restore(save)
})
```

Use the read-only view for a HUD. `program.clock` is useful for ordering, but
the view is the source of display names:

```ts
const current = await Effect.runPromise(SubscriptionRef.get(player.session.current))
const world = await Effect.runPromise(player.program.view(current.state))
const clock = world.clock
const label = clock == null
  ? "No clock"
  : `${clock.weekdayLabel ?? `Day ${clock.day}`} — ${clock.slot ?? "slot"}`
```

Next, read [the clock](/language/clock/), [playing a story](/tooling/play/), and
[the runtime contract](/tooling/runtime-contract/) for the language and host
surfaces used here.
