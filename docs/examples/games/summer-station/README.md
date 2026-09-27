# Summer Station

A seven-day visual novel at a mountain weather station: an intern's summer week of morning readings, a forecast the village needs by Friday, a meteor shower on Thursday night, two people to fall for, and a lantern festival on Sunday that ends the summer.

**Genre:** visual novel / dating sim on a clock (the engine owns the day, the slot and the weather; Lute owns who is where and what plays).

## The game

The player arrives at Hallen Ridge Weather Station on Monday morning and works for Dr. Ines Halvorsen. Each day has a morning, an afternoon and a night; every slot opens with the station routine, then whatever the day holds. People are only in certain places at certain times: Sol keeps the radio room in the morning and the roof at night, Wren is at her goat pen in the village in the afternoons, and Pell brings the post up on Tuesdays and Fridays. The engine rolls the weather; a storm shuts the path to the village and sends the player round the station lashing things down. Two routes (Sol, Wren) build on affection and meet at a Saturday-night confession; the Sunday festival reads everything the week did.

- 14 scenes: the arrival (`days.arrival`), the daily morning readings, the forecast and its delivery, the radiosonde, five for Sol (first meeting, the roof, the Perseids, the morning after missing them, the confession), three for Wren (first meeting, the confession, Sunday on the deck), and the festival.
- 24 beats and 11 entries in 7 lore bundles (the day's events, the routine, conversations, storm chores, the meteor count, the logbook, empty places).
- 5 quests: the festival forecast (due Friday), Wren's lantern (three parts, before Sunday), the Perseids (Thursday), the always-on logbook job, and a batten-down quest that comes back with every storm.
- It runs one week (`days: 7`). It ends at the festival on Sunday night, which sets `run.summerOver` (schema `terminal:`); the festival plays differently for the Sol route, the Wren route and neither, and for whether the forecast and the lantern made it.

## What it shows

- **Declared clock with a week** — three slots, `raise: { slot: slotStart, dayEnd: dayEnd }`, `raiseAtStart: true` (the engine raises `slotStart` at run start, which presents `days.arrival`; asserted at step 1 of `plays/week/mon-wed.steps.yaml`), `week:` labels, `clock.weekday` in `<match>` and defs (`@postDay`), `{{clock.weekdayLabel}}`, `{{run.day:ordinalWord}}`: `world.schema.yaml`, `scenes/routine/morning.lute`, `lore/routine.lute`.
- **A `sequence` occasion** — `slotStart` plays the routine (priority 10) and then the day's events; a storm (priority 20) goes first: `lore/days.lute`, `lore/routine.lute`, `plugins/game.occasions/occasions/game.yaml`.
- **Schedules as rules** — derived `at(person, place)` from `cel()` guards on the clock, `not pathShut(village)` over a `reserved:` engine relation, derived `ready(P)` over `run.aff[P]`, cast `present:` read from them: `world.schema.yaml`; `lute calendar --facts at` prints who is where, when.
- **Sub-kinds and per-member state** — `suitor: { subsetOf: person }`, `run.aff` with `per: suitor`: `world.schema.yaml`, `scenes/sol/*.lute`, `scenes/wren/*.lute`.
- **Seasons** — `seasons: storm: { live: "@stormLive" }`, `season.storm.*` state, `once="season:storm"` chores, a `tier="season:storm"` quest with `start="@stormLive"`, `prev.season.storm.mast` in the second storm: `lore/storm.lute`, `quests/station.lute`, `plays/two-storms.play.yaml`, `tests/storm-second.test.yaml`.
- **Spend periods and side remarks** — `once="day"` affection on `also` beats (`wrenWarm`, `solWarm`), `once="false"` conversations, `once="run"` meteor record, `spentBy: "run.balloonUp"` for a scene retried every afternoon until it succeeds: `lore/talks.lute`, `lore/perseids.lute`, `scenes/station/balloon.lute`, `plays/once-a-day.play.yaml`, `plays/balloon.play.yaml`.
- **Per-member beats** — `for="kind:suitor"` Sunday note presented once per suitor it holds for (`days.note for sol`); `target="kind:place"` beats with `holds(at(sol, occasion.target))` and `{{occasion.target}}` rendering place `labels:`: `lore/days.lute`, `lore/talks.lute`, `lore/storm.lute`, `tests/sunday-note.test.yaml`.
- **Plugin occasions** — `place.*` targets, a typed `payload` (`shower { count }`) with `raisedWhen`, `select: all` for the logbook (entries picked with `pick:`), `select: first` elsewhere: `plugins/game.occasions/occasions/game.yaml`, `lore/perseids.lute`, `lore/logbook.lute`.
- **Plugin directive with declared effects** — `::gather{part=…}` asserts `carrying(@part)`; a `KEEPSAKE` reward kind: `plugins/game.occasions/directives/game.yaml`, `lore/talks.lute`, `quests/festival.lute`.
- **Quests and missed events** — `::accept`, `by=` deadlines, an objective completed `on="visit" target="place.village"` with `count(carrying(_)) >= 3`, a `visited('sol.meteor')` objective, an `optional` objective, `questComplete`/`questFailed` handlers; scenes that read `quest.<id>.state == 'failed'` or `!visited(…)`: `quests/`, `scenes/sol/missed.lute`, `scenes/festival/night.lute`, `lore/routine.lute`.
- **Templates (components)** — `enter` and `reading` (an enum param defaulting to `@sky`, matched inside), and `inesAt`, a beat template whose header writes `on`/`target`/`priority`/`when` from params (`holds(at(ines, @where))`) and places `::body` after a nested `::use`: `components/`, used by `<beat use="inesAt">` in `lore/talks.lute`.
- **Menus** — `<branch>` choices with `when=`, a `<hub>` with `once`/`when`/`exit` choices; the two confession menus `solAnswer` and `wrenAnswer` chosen by id in plays: `scenes/sol/confession.lute`, `scenes/wren/confession.lute`, `lore/talks.lute`.
- **User tier and the last run** — `user.summers`, `prev.run.route` in the arrival: `scenes/days/arrival.lute`, `tests/arrival-second-summer.test.yaml`.
- **Own vocabulary** — a project `vocabulary.schema.yaml` with camelCase members (`fadeInUp`, `turnAway`, `whiteOut`).
- **Tests and plays** — 10 scenario tests (`tests/`) and 7 plays (`plays/`); the route plays share Monday to Wednesday through `include: week/mon-wed.steps.yaml`, and `wren-weekend`/`balloon` start from saves.

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/summer-station
lute test docs/examples/games/summer-station                          # 10 tests + 7 plays
lute play docs/examples/games/summer-station --script docs/examples/games/summer-station/plays/sol-route.play.yaml
lute play docs/examples/games/summer-station --script docs/examples/games/summer-station/plays/two-storms.play.yaml
lute beats docs/examples/games/summer-station --occasion visit --target place.roof
lute calendar docs/examples/games/summer-station --axis run.day=1..7 --axis run.slot=morning,afternoon,night --occasion slotStart --facts at
lute calendar docs/examples/games/summer-station --occasion shower --axis occasion.payload.count=1,41,63 --axis run.slot=night
lute trace docs/examples/games/summer-station/scenes/routine/morning.lute --state run.day=7 --state run.weather=fog --choose care=careful
```

## Origin

Written as a dogfood project for Lute 0.23 by a visual-novel writer persona; it stress-tested time as a first-class thing (a week clock, weekdays, once-a-day spends), who-is-where schedules as Datalog rules, deadlines and missed events, and later seasons and kind-targeted beats.
