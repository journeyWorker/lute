# Monster League

A monster-collecting RPG across the Verdane region: eight gyms, a villain team, a rival, the Elite Four and a postgame. Five writers built it in one project.

**Genre:** creature-collector RPG (Gold/Silver-style day/night clock, weekday events), a narrative layer over an engine that owns maps, battles and catches.

## The game

The player picks a starter from Professor Alder in Hollowtown, then crosses Verdane in map order: Southshore (Stone and Tide gyms, the Moonstone dig, the museum), Eastmarch (Spark and Bloom gyms, Ash Tower, the power plant), the Midlands (the Game Corner hideout, Helix Tower, Mind and Venom gyms, a sleeping giant, the bug-catching contest) and the Highlands (Cinder Isle, Earth Gym, Victory Road). Team Eclipse has a lair in each part of the map (`hidden` → `found` → `cleared`). Kai, the rival, turns up six times. Key items and field moves (Flash, Cut, Surf, Strength, the Spectral Lens, the Waking Flute) open the gates between the areas.

The main line ends on the League plateau: the Elite Four (the streak resets on a loss), the Champion, the Hall of Fame, and the next morning at home (`scenes/spine/epilogue.lute`). There is no `terminal:`. The game keeps going into the postgame: the Rift Cavern legendary, and the Sunfall Isles, where three island beacons are relit and Admin Nyx waits in the shrine.

Size: 67 scenes, 65 lore documents, 42 quests, 7 shared components, 151 species and 276 cast entries. The world is split into areas: `spine` (lead), `south`, `east`, `mid`, `north` and `isles`. Each area owns its own `scenes/`, `lore/`, `quests/`, `tests/` and `plays/` subtrees. `AREAS.md` is the ownership contract the lead gave the area writers. It was written against Lute 0.25–0.27, so some of its syntax is out of date, but the design is current.

## What it shows

- **Engine plugin pack**: `plugins/league.engine/plugin.yaml` exports occasions, events, reward kinds, directives, a bridge, state and cast from one capability plugin.
- **Occasions**: `plugins/league.engine/occasions/league.yaml` declares 18 occasions. Most are targeted over closed kinds (`talk@npc.<person>`, `enterTown@town.<town>`, `challenge@arena.<arena>`, `caught@mon.<species>`). The file also shows `select: sequence` against `select: first`, `judge: before` on `hallOfFame`, and `raisedWhen: "@champion"` on the postgame ferry `sail`.
- **Chapters**: `lute.project.yaml` puts `spine.opening` and then `spine.home` into the one `newGame` raise. `scenes/spine/epilogue.lute` is not in the chain: it has its own `on: dayStart` plus `after: 'visited("spine.hallOfFame")'`.
- **Clock**: `schema/world.schema.yaml` (`clock:`) defines morning/day/night slots, a Monday-to-Sunday week with labels, and engine raises mapped to `slotStart`/`dayStart`/`dayEnd`. Weekday defs such as `contestDay` and `weekend` drive the market, the contest, lantern night and the Battle Tower.
- **Bridge and payloads**: in `plugins/league.engine/bridge/battle.yaml`, `::battle` hands a fight to the engine and waits for `{won, prize, turns, fainted}`. Content reads the result through `@wonFight` and `<match on="scene.battle.fight.won">` (`scenes/spine/elite-morwen.lute`).
- **Reward kinds**: `plugins/league.engine/rewardkinds/league.yaml` defines `MONEY`, which credits `run.money`, and `ITEM` and `TM`, which require a target.
- **Facts and rules**: `schema/species.schema.yaml` holds 151 species and 211 app-tier `speciesType` facts. `schema/world.schema.yaml` (`relations:`, `rules:`) derives `canPass(<gate>)` from badges and key items. Any gate can be traced back to the scene that produces it (see `lute scenario … knowledge` below).
- **Multi-author schemas**: `lute.project.yaml` `defaults.uses` globs `schema/areas/*.schema.yaml`. Each area adds its roster members with `add:` and declares its own prefixed state and sub-kinds (`subsetOf`) there (`schema/areas/south.schema.yaml`). Shared components are listed once in `defaults.components`.
- **Components and beat templates**: there are 7 shared components in `components/`. `trainerBattle` is used about 160 times as `<beat use="trainerBattle" …/>` (`lore/south/route3.lute`). `beaconKeeper` (`components/beacon-keeper.component.lute`, used in `lore/isles/islands.lute`) takes a def as a param default (`lit: { default: "@islesLit" }`).
- **Cast with intended shared names**: in `plugins/league.engine/cast/*.yaml`, one cast file per area, the role names "Eclipse Grunt", "Gym Guide", "Eclipse Scientist" and "Dusk Grunt" are marked `sharedName: true`.
- **Quests**: `quests/spine/league.lute` holds the main line, the badge road, the Eclipse arc, the Elite Four, the rivalry, a Dex chain (`dex10` → `dexFull`), a postgame `legacy` with `complete="any"`, and a board quest. Each area has its own side quests in `quests/<area>/`. `quests/isles/beacons.lute` has one subquest per island, a courier objective with `by="@isNight"`, and a weekly `rearm="clock.weekday == 0"` tower quest.
- **spentBy**: `lore/isles/islands.lute` (the windmill beat, `spentBy="run.islesVanes >= 3"`).
- **Per-member beats**: `lore/isles/radio.lute` (`for="kind:island"`, a `dayStart` radio report for each relit island).
- **Hubs, branches, choices**: `scenes/east/portvolt-market.lute`, `lore/mid/ghosts.lute`, `lore/north/verdantcity.lute`, `scenes/spine/home.lute`.
- **Plays with included steps**: `plays/spine.play.yaml` runs the whole game through `include:` of each area's `plays/steps/<area>*.steps.yaml`, and each area's last step is its hand-off `expect:`. Each area also has a full playthrough of its own (`plays/<area>.play.yaml`).
- **Scenario tests**: 108 tests in `tests/<area>/`, one directory per owner.

## Run it

From the repo root:

```sh
lute check-project --deny-warnings docs/examples/games/monster-league
lute test docs/examples/games/monster-league
lute play docs/examples/games/monster-league --script docs/examples/games/monster-league/plays/spine.play.yaml
lute play docs/examples/games/monster-league --script docs/examples/games/monster-league/plays/isles.play.yaml
lute beats docs/examples/games/monster-league --occasion talk --target npc.profAlder
lute scenario docs/examples/games/monster-league knowledge --for blockAshTowerStairs
lute calendar docs/examples/games/monster-league --axis run.day=1..7 --axis run.slot=morning,day,night --occasion enterTown
```

`lute test` runs 108 scenario tests and 8 plays. The 211 species facts make every load slow (several seconds). This is a known cost of static facts at this scale.

## Origin

This was the round-4 dogfood project for Lute 0.25–0.28. A lead agent wrote the world contract and the spine, and four area writers (plus a fifth for the postgame isles) filled in their areas in parallel. It stress-tested multi-author projects: splitting schemas and cast by area, id collisions, cross-area gates, and load time at 800+ beats with a large fact base.
