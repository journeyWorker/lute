---
title: Build a town game
description: Build a small, clock-driven town with Lute and player-web — endings, a second play-through, and a Korean translation.
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
  user.endings: { type: int, default: 0 }
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
the view supplies the slot and weekday labels. `user.endings` is the one
user-tier path: it outlives a run, and [step 7](#7-a-second-play-through) uses it.

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

The `start` beat introduces the town. Its lines carry a stable `code`, which
`lute tag` writes once (see [step 8](#8-translate-quayside)); the gated lines
greet a returning player (see [step 7](#7-a-second-play-through)):

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
@june{code="0010" when="has(prev.run.ending) && prev.run.ending == 'restore'"}: The hall's lanterns burned all night. Welcome back.
@june{code="0020" when="has(prev.run.ending) && prev.run.ending == 'retire'"}: The old hall sleeps, but the town remembers you.
@june{code="0030"}: The tide is out, and Quayside is already awake.
@narrator{code="0010" when="user.endings > 0"}: Bell answers given so far: {{user.endings}}.
@narrator{code="0020"}: A hand-painted sign points toward the square, the pier, and the bakery.
```

A visit beat can offer a choice and write the ending. Each ending choice also
counts the answer in the user tier:

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
@june{code="0010"}: Every stall has something to mend, trade, or remember.
::set{run.visits += 1}

<branch id="closingBell" prompt="The harbor bell rings. What will you do with the old town hall?">
  <choice id="restore" text="Restore it as a community hall">
    ::set{run.ending = 'restore'}
    ::set{user.endings += 1}
    @june{code="0020"}: We will fill it with lanterns and neighbors.
  </choice>
  <choice id="wait" text="Wait for the bell to ring again">
    @june{code="0030"}: We can take one more evening before deciding.
  </choice>
  <choice id="retire" text="Let the building rest with the tide">
    ::set{run.ending = 'retire'}
    ::set{user.endings += 1}
    @june{code="0040"}: Some doors close gently, and that is a kind of care.
  </choice>
</branch>

<match subject="run.ending">
  <when is="restore">
    @narrator{code="0010"}: The square keeps its lights on for the hall.
  </when>
  <when is="retire">
    @narrator{code="0020"}: The square grows quiet around the old hall.
  </when>
  <when is="none">
    @narrator{code="0030"}: The bell waits for your answer.
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
@narrator{code="0010"}: At evening tide, the town lights come on one by one.
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
  @june{code="0010"}: Fresh bread, repaired nets, and one more story before sunset.
</entry>

<entry id="pierBark" category="bark" on="visit" target="place.pier" priority="-1">
  @tom{code="0010"}: Mind the loose board. The pier remembers every storm.
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

Bindings resolve paths below `assetsBase`. `ui.title.text` names the game on
the title screen; `ui.title.music` uses the `assets:` key `title-music`;
`ui.ended` reads `run.ending` and selects a screen for each named ending;
`flow.newGame` raises `start` each time New game begins a run, so the host
never raises it itself:

```yaml check="docs/examples/quayside/player.bindings.yaml"
bindingsVersion: "0.48"
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
  title: { background: bg/title.png, music: title-music, text: "Quayside" }
  ended:
    by: run.ending
    screens:
      restore: { title: "The Hall Opens", text: "Quayside gathers under one roof.", background: bg/restore.png }
      retire: { title: "A Gentle Closing", text: "The town lets the old hall rest.", background: bg/retire.png }
flow:
  newGame: { occasion: start }
```

Every referenced placeholder file is committed under `docs/examples/quayside/assets/`.
The manifest uses `bindingsVersion: "0.48"`; see the [bindings reference](/player/bindings/)
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
the essential pattern is:

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

## 7. A second play-through

An ending closes a run, not the game. A **new run** (`newRun`) resets every
`run.*` path to its default — so `run.ending` is `none` again and `terminal`
no longer holds — while the **user tier** carries over: `user.*` values,
user-tier facts, `once: user` spends and `entry.<id>.everRead` (the per-run
`entry.<id>.read` resets). Quayside
keeps one user-tier counter, `user.endings` (declared in step 1), and both
ending choices in `market` add one to it (`::set{user.endings += 1}`).

`newRun` also snapshots the run that ended: every `run.<path>` can be read as
[`prev.run.<path>`](/state/state-model/#the-previous-run), the value it had
when that run ended. You declare nothing for it, content cannot write it, and
it is `unset` until a run has ended, so every read needs a `has(…)` guard. The
opening in step 2 uses both tiers: June's first two lines are gated on
`prev.run.ending`, and the narrator line is gated on `user.endings` and
prints it. In the first run none of them plays.

A beat (`once: user`) or entry (`once="user"`) that should play once per
player rather than once per run stays spent across new runs; see
[entry beats](/tooling/play/#entry-beats). Quayside does not need one.

A play script starts the second run with a `newRun` step and then raises
`start` again:

```yaml check="docs/examples/quayside/plays/second-run.play.yaml"
# Finish a run, start a new one, and hear Quayside remember the first.
steps:
  - occasion: start
    expect: { winner: opening }
  - occasion: visit
    target: place.square
    choose: { closingBell: restore }
    expect: { winner: market, state: { run.ending: restore, user.endings: 1 } }
  - newRun: true
    expect: { state: { prev.run.ending: restore, run.ending: none, user.endings: 1 } }
  - occasion: start
    expect: { winner: opening }
expect:
  end: complete
  state: { prev.run.ending: restore, user.endings: 1 }
  transcriptContains:
    - "@june: The hall's lanterns burned all night. Welcome back."
    - "@narrator: Bell answers given so far: 1."
```

`lute play --project . --script plays/second-run.play.yaml` shows what the new run took
from the old one and what the opening says this time (steps 3–4 of the
output):

```text
── step 3 · new run ──────────────
  run.* state, run-tier facts and once: run reset; prev.run.* holds the ended run (4 values)
  prev.run.day = 1
  prev.run.ending = "restore"
  prev.run.slot = "morning"
  prev.run.visits = 1
── step 4 · start ──────────────
  ✓ opening [scene, priority 10]
  → opening
  match -> arm 1
::bg{location="quayside" time="morning"}
@june: The hall's lanterns burned all night. Welcome back.
  skip @june "The old hall sleeps, but the town remembers you." — when: false
@june: The tide is out, and Quayside is already awake.
@narrator: Bell answers given so far: 1.
@narrator: A hand-painted sign points toward the square, the pier, and the bakery.
── end: complete (4 steps) ──────────────
```

`lute test --project docs/examples/quayside` runs it with the other plays.

In player-web the second run needs no host code beyond step 5's raises, and it
survives a reload. The host raises `visit` for `place.square` when the player
walks onto the square on its map
(`player.send({ type: "raiseOccasion", occasion: "visit", target: "place.square" })`);
`newRun`, the lineage and the `start` raise (`flow.newGame`, step 4) happen
inside the player:

1. **Run 1.** New game raises `start`. Walk to the square and choose "Restore
   it as a community hall". `terminal` holds, and the player shows the
   `restore` ending screen ("The Hall Opens").
2. **The lineage is written.** When the session presents that `ended` output,
   the player takes the runtime snapshot — the **lineage**: the user tier plus
   the ended run's values — and stores it in its profile at once, replacing
   the previous one.
3. **New game.** Back to title (no confirm on an ended run), then New game.
   The session is at `ended`, so New game sends `newRun` and then raises
   `start`; the opening greets you with the `prev.run` line and "Bell answers
   given so far: 1."
4. **Reload, then New game.** The fresh session is untouched and cannot send
   `newRun`, so New game restores the profile's lineage first, sends `newRun`
   and raises `start`: the same two lines play. A run in progress is not part
   of the lineage; only a save (quick, auto or a slot) keeps it.
5. **Abandon a run, then New game.** Abandoning a run through the menu and
   starting New game does the same: the lineage is restored and `newRun` sent,
   so anything that run changed, user tier included, is dropped.

A host that mounts with `snapshot` or `session` has chosen its state: endings
still write the lineage, but New game never restores it. A host `seed` is not
used on New game while a lineage exists. The full rules are in
[custom hosts: the cross-run model](/player/custom-hosts/#second-play-throughs-the-cross-run-model-047).

## 8. Translate Quayside

Name the source language in the manifest. The compiled project index carries
it as `contentLang`, and the player uses it as the source locale:

```yaml check="docs/examples/quayside/lute.project.yaml"
pluginsDir: plugins/
defaultProfile: game
profiles:
  game:
    plugins: { quayside.occasions: true }
defaults:
  contentLang: en
  uses: [world.schema.yaml]
```

The round trip is four commands, run in `docs/examples/quayside`:

```console
$ lute tag .
$ lute loc export --project . -o locales/ko.json
$ lute loc import locales/ko.json -o locales/bundle.json
$ lute compile --project . -o bundle/ --locale-bundle locales/bundle.json
```

**Tag.** A line's `lineId` is built from its `code`. A line without one exports
with `lineId: null`, and `loc import` skips it, so `lute tag .` comes first: it
wrote the `code="0010"`, `code="0020"`… attributes in step 2 (per speaker, in
document order) and leaves existing codes alone. Once a translation exists,
keep the codes stable.

**Export.** `lute loc export` writes one row per line, option label and
prompt, in document order. The `closingBell` prompt is its own row,
`kind: "prompt"` with the key `closingBell.prompt` and the lineId
`market.closingBell.prompt`, just before the rows of its three options
(`market.closingBell.restore`, …). Every row carries a `sourceHash`: the
SHA-256 of the record's plain source text. `--format csv` writes the same
columns, with `sourceHash` last.

**Translate.** The file stem is the locale tag, so `locales/ko.json` is the
`ko` translation. Replace each `text` and nothing else: keep `lineId` and
`sourceHash`, and keep placeholders such as `{{user.endings}}` and inline
modifiers. Two rows of the committed
[`locales/ko.json`](https://github.com/journeyWorker/lute/tree/main/docs/examples/quayside/locales/ko.json)
(hashes shortened here):

```json
  {
    "code": "0010",
    "file": "./scenes/market.lute",
    "kind": "line",
    "line": 14,
    "lineId": "market.june_0010",
    "source": null,
    "sourceHash": "sha256:058c16387037…",
    "speaker": "june",
    "text": "가게마다 고치거나, 바꾸거나, 기억할 것이 있어."
  },
  {
    "file": "./scenes/market.lute",
    "key": "closingBell.prompt",
    "kind": "prompt",
    "line": 17,
    "lineId": "market.closingBell.prompt",
    "source": null,
    "sourceHash": "sha256:bbea0ecc733d…",
    "text": "항구의 종이 울립니다. 옛 마을 회관을 어떻게 할까요?"
  },
```

**Import.** `lute loc import` turns the translated rows into a locale bundle
(`schemaVersion: 2`): the texts per lineId and locale, and beside them the
`sourceHash` each translation was made from. Importing the same rows again
writes the same bytes. An excerpt of
[`locales/bundle.json`](https://github.com/journeyWorker/lute/tree/main/docs/examples/quayside/locales/bundle.json)
(one entry, compacted, hash shortened):

```json
{
  "schemaVersion": 2,
  "locales": ["ko"],
  "entries": {
    "market.closingBell.prompt": { "ko": "항구의 종이 울립니다. 옛 마을 회관을 어떻게 할까요?" }
  },
  "sourceHashes": {
    "market.closingBell.prompt": { "ko": "sha256:bbea0ecc733d…" }
  }
}
```

**Compile.** `compile --project <DIR> --locale-bundle <FILE> -o <DIR>` merges the bundle into every artifact:
lines and option labels gain a `texts` map, and the `closingBell` branch gains
`promptTexts` next to its source `prompt` and `promptLineId`. The source text
is never replaced. A record with no translation for a locale the bundle
declares is `W-L10N-MISSING`, one per lineId and locale, and keeps its source
text. Quayside translates every row, so it compiles clean; a partial
translation warns for each pair it has not reached yet. Ship complete
translations, and let CI refuse a gap with `--deny W-L10N-MISSING`.

**When the source changes.** Edit June's first market line after the export
(say, to "Every stall has something to mend or trade.") and compile again. The
Korean text is still merged, but its stored hash no longer matches the new
source text:

<!-- lute-diagnostics -->
```text
./scenes/market.lute:1:1: warning [W-L10N-STALE] `ko` text for `market.june_0010` was translated from an older source text; re-export with `lute loc export` and update the translation
```

Re-export, update that row's `text` (the new row carries the new
`sourceHash`) and import again. `--deny W-L10N-STALE` or `--deny-warnings`
makes the compile refuse while a translation is stale. Locale bundles do not
change whole-project `lute check` or `lute test`: plays run on the source text.

**In the player.** Serve the `bundle/` compiled with `--locale-bundle` as the
`bundleUrl` from step 5. The Settings screen's Locale row lists the source
locale first — `en`, from `contentLang` — and then the compiled locales, here
`ko`; a new profile starts at `en`. Pick `ko` mid-run: the next line, the
choice labels, the `closingBell` prompt and the backlog read Korean, without a
new game. Speaker names are not localized. The locale is a setting
(`settings.locale`), not a mount option; see
[custom hosts: locale](/player/custom-hosts/#locale-047).

Next, read [the clock](/language/clock/), [playing a story](/tooling/play/),
[the state model](/state/state-model/), [the `lute loc` commands](/tooling/cli/#loc-export)
and [the runtime contract](/tooling/runtime-contract/) for the language and
host surfaces used here.
