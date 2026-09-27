# Harbor Days

The content side of a small live-ops fishing game: a two-week event on a harbor, with a weekly market, villager birthdays, a lantern festival that can switch on and off, and a storm that can end it early.

**Genre:** live-ops / cozy life sim (event content for a mobile game; the engine owns the loop, Lute owns what plays).

## The game

The player arrives, pulls in a first net with Tomas, and agrees (or not) to help Mara with the lantern festival. After that the engine drives: each day has dawn, noon and dusk; Monday noon is market day; villagers can be talked to, fish are landed with a weight, the boat Gull can be launched once you are allowed to sail. When the operator switches the festival on, a lantern season opens with its own state and quest, and it resets the next time it opens.

- 8 scenes: a three-scene opening chain (`arrival` → `firstNets` → `harborMeet`) plus five scenes the engine's occasions pick (`hub.idle`, `lantern.hang`, `birthday.song`, `market.weekly`, `storm.hit`).
- 9 beats and entries in one lore bundle (villager talks, landed fish, the Gull launch, lantern night, the dawn tide).
- 3 quests: a weekly catch that rearms every Monday, a per-festival lantern drive, and a one-off marlin hunt with a deadline.
- It runs 14 days. It ends when the clock runs out, or on day 13 if a squall was forecast and wrecks the moorings (both via schema `terminal:`).

## What it shows

- **Finite clock with a week** — slots, `raise: { slot: tide, dayEnd: nightfall }`, `week:` with labels, `last: { day: 14 }`, `clock.ended` in `terminal:`: `world.schema.yaml`; `plays/fair-weather.play.yaml`, `plays/storm.play.yaml`.
- **Seasons** — `seasons: lanterns: "@festivalLive"`, `season.lanterns.*` state, a season-tier relation (`wishedOn`) reset per window, `prev.season.*` from the last window: `world.schema.yaml`, `scenes/lantern-hang.lute`, `plays/festival.play.yaml`.
- **Spend periods** — `once: week`, `once: season:lanterns`, `once="day"`, `once: user`, `once="false"`: `scenes/weekly-market.lute`, `scenes/lantern-hang.lute`, `lore/talks.lute`, `components/villager-chat.component.lute`.
- **`spentBy` beside a stated `once:`** — `launchGull` is spent for the run once a marlin is caught, even after `::sell` retracts the fact; `once="run"` says the latch is intended: `lore/talks.lute`, `tests/launch.test.yaml`, step 13 of `plays/week-one.play.yaml`.
- **Plugin occasions** — targets (`npc.*`, `fish.*`, `boat.*`), a typed `payload` (`landed { weight }`), `raisedWhen` (`launch`, `lanternNight`), `select: all`, `select: sequence`, `judge: before`: `plugins/game.occasions/occasions/game.yaml`.
- **Plugin directives with declared effects** — `::haul` asserts, `::sell` retracts and writes, `::hangLantern` writes season state: `plugins/game.occasions/directives/harbor.yaml`.
- **Per-member beats** — `for: "kind:villager"` plays the birthday once per villager with a birthday fact; `target="kind:fish"` / `kind:villager` kind beats with `{{occasion.target}}` and entity `labels:`: `scenes/birthday.lute`, `lore/talks.lute`, `plays/birthdays.play.yaml`.
- **Facts and relations** — run-tier and season-tier relations, a `reserved:` engine-owned relation, `holds(...)` in guards: `world.schema.yaml`.
- **Templates (components)** — `villagerChat` with typed params, a header that writes `once`/`when`/`after` from params (`user.bond[@who]`), `::body`: `components/villager-chat.component.lute`, used by `<beat use=…>` in `lore/talks.lute`.
- **Quests** — `start=`/`rearm=` on a def, `tier="season:lanterns"`, a `by=` deadline: `quests/harbor.lute`.
- **Chapters and defs** — `chapters:` for the opening, `defs:` (`@festivalLive`, `@monday`), `questTier: run` default: `lute.project.yaml`, `world.schema.yaml`.
- **Own vocabulary** — a project `vocabulary.schema.yaml` with camelCase members (`fadeInUp`, `fadeOut`).
- **Tests and plays** — 6 scenario tests (`tests/`) and 5 plays (`plays/`).

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/harbor
lute test docs/examples/games/harbor                           # 6 tests + 5 plays
lute play docs/examples/games/harbor --script docs/examples/games/harbor/plays/week-one.play.yaml
lute play docs/examples/games/harbor --script docs/examples/games/harbor/plays/festival.play.yaml
lute beats docs/examples/games/harbor                          # every occasion's candidates in order
lute calendar docs/examples/games/harbor --axis clock --occasion tide
lute calendar docs/examples/games/harbor --occasion landed --axis occasion.payload.weight=0,40
```

## Origin

Written as a dogfood project for Lute 0.27 by a live-ops game designer persona; it stress-tested seasons, spend periods, `spentBy`, plugin occasions with targets and payloads, directive effects, per-member beats and templates all in one project.
