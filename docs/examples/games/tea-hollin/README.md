# Tea at Hollin Street

A one-afternoon cozy mystery: Aunt Mabel's heirloom spoon is missing, and Pippa has until evening tea to name the thief.

**Genre:** cozy mystery, short linear investigation with a deadline.

## The game

Pippa arrives at Number nine Hollin Street, where the tea table has six cups, six saucers and five spoons. She searches the parlour, questions the cook below stairs (only after morning), and in the rose garden names a culprit: the cook, the vicar, or — once she has found two clues — the magpie on the sundial.

- 6 scenes: a four-scene chapter chain (`arrival` → `parlour` → `kitchen` → `garden`) and two endings.
- One day of three slots (morning, afternoon, evening); the kitchen opens in the afternoon, and the case must be solved before evening.
- Two endings: `ending.found` (the magpie's nest) and `ending.wrong` (cold tea).
- One quest, `findSpoon`, that completes, fails on a wrong accusation, or fails when evening arrives.
- About ten minutes of play.

## What it shows

The smallest project in the set, written the way the getting-started guides teach it.

- **Chapters** — a single chain on the `chapter` occasion: `lute.project.yaml`.
- **Endings off the chain** — each ending is its own scene with `on: chapter`, `after: 'visited("garden")'` and exclusive `when:` guards: `scenes/ending/found.lute`, `scenes/ending/wrong.lute`.
- **Scene gate on the clock** — `when: "run.slot != 'morning'"` keeps the kitchen closed until the afternoon: `scenes/kitchen.lute`.
- **Branch and hub** — a branch that records its pick with `into=`/`value=` into an enum path, a guarded choice (`when="run.cluesFound >= 2"`), a hub with `once` choices and an `exit` choice: `scenes/parlour.lute`, `scenes/garden.lute`, `scenes/kitchen.lute`.
- **Line guards and interpolation** — `@pippa{mono when="run.sawPocket"}`, `{{run.slot}}`: `scenes/garden.lute`, `scenes/ending/wrong.lute`.
- **Clock** — one day, three slots, raising `teatime` on each `advance:`: `world.schema.yaml`.
- **Lore entry** — `once="slot"` on the clock occasion: `lore/kettle.lute`.
- **Quest** — `tier="run"`, an `optional` objective, a `by=` deadline on the clock slot, `fail=` on the accused enum, `questComplete`/`questFailed` handlers: `quests/spoon.lute`.
- **Tests and plays** — 7 scenario tests (`tests/`) and 3 plays covering the good ending, a wrong accusation and a missed deadline (`plays/`).

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/tea-hollin
lute test docs/examples/games/tea-hollin                       # 7 tests + 3 plays
lute play docs/examples/games/tea-hollin --script docs/examples/games/tea-hollin/plays/magpie.play.yaml
lute calendar docs/examples/games/tea-hollin --axis clock      # when the kettle sings, where the chain stands
lute trace docs/examples/games/tea-hollin/scenes/kitchen.lute --choose askCook=scullery,leave
```

## Origin

Written as a dogfood project for Lute 0.27 by a first-time-writer persona who had never programmed; it stress-tested the getting-started path — YAML quoting, tag flags, the chapter chain, the clock and quest deadlines — and every slip it logged became a diagnostic or doc fix.
