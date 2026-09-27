# Example games

Fourteen complete games written against Lute 0.21–0.28 as dogfood projects, each by an agent
playing a different kind of writer, and migrated to the current language. Each directory is its own
project (`lute.project.yaml`, schemas, scenes, lore, quests, plugins, `tests/`, `plays/`) with a
README: the premise, what it shows of the language with the file to look at, and how to run it.

CI checks every game with `lute check-project --deny-warnings` and runs its tests and plays
(`lute test docs/examples` recurses into each one). Where a game means something a warning would
otherwise question, it says so in the spelling Lute provides for that intent, not with a
suppression.

New to Lute? Start with [`tea-hollin`](tea-hollin/) (the smallest, written the way the
getting-started guides teach). Coming from Ink or Yarn, read [`ledger`](ledger/). For scale, see
[`monster-league`](monster-league/).

| Game | Title | Genre | What it shows |
|---|---|---|---|
| [`ashen-stair`](ashen-stair/) | The Ashen Stair | roguelike hub (Hades-like) | the run boundary and `prev.run.*`, engine-owned state and reserved relations, reward kinds, a season, per-NPC send-offs with `for=` |
| [`drowned-crown`](drowned-crown/) | The Drowned Crown | roguelike (Hades-like) | run and user tiers, a permanent ending (`terminal: { when, persists: true }`), beat ladders on the hub visit, occasion payloads |
| [`ember-road`](ember-road/) | Ember Road | party RPG chapter | companion approval and faction reputation, a day clock of nights on the road, a `::check` dice bridge, battle payloads |
| [`harbor`](harbor/) | Harbor Days | live-ops life sim | a finite clock with a week, a festival season, spend periods, `spentBy` beside `once:`, directive effects, per-villager birthdays |
| [`hollow-ward`](hollow-ward/) | Hollow Ward | survival horror / escape | rules as the door graph with `raisedWhen:`, blocking bridges, entries, terminal fates, a large test suite |
| [`lamplight`](lamplight/) | Lamplight | detective mystery / time loop | facts and stratified Datalog rules, user-tier evidence surviving each loop, a casebook `series:`, quests from sub-quests |
| [`lantern-academy`](lantern-academy/) | Lantern Academy | otome / dating sim with NG+ | route locks as rules, `raiseAtStart: true`, user-tier `cleared` facts across terms, date and exam bridges |
| [`ledger`](ledger/) | The Lighthouse Keeper's Ledger | interactive fiction (Ink port) | Ink mapped to Lute: a hub with `<return>`, visit counters, `terminal:` as `-> END`; the Ink source beside it |
| [`lighthouse-keeper`](lighthouse-keeper/) | Skerry Rock | deduction mystery (Obra Dinn-like) | stratified rules with `excludes:`, conclusions derived rather than asserted, a clock with `raiseAtStart`, an inquest chapter chain |
| [`monster-league`](monster-league/) | Monster League | creature-collector RPG | scale: five writers, per-area schemas, 18 occasions, 151 species as facts, a template used ~160 times, `sharedName: true` |
| [`seven-days`](seven-days/) | Seven Days in Marrow Bay | day-clock visual novel | deadlines as clock positions, a derived timetable relation, `newRun` and `prev.run.*`, three terminal endings |
| [`starfall-gacha`](starfall-gacha/) | Starfall | live-service gacha (story layer) | three seasons with reruns, the content calendar as defs, rarity sub-kinds, summon payloads, `share=` |
| [`summer-station`](summer-station/) | Summer Station | visual novel / dating sim on a clock | schedules as rules, a `select: sequence` routine plus events, a storm season, sub-kinds with `per:` state |
| [`tea-hollin`](tea-hollin/) | Tea at Hollin Street | cozy mystery | the smallest: one chapter chain, a branch and a hub, a one-day clock, a quest with a deadline |

Check and test one game:

```sh
lute check-project --deny-warnings docs/examples/games/<slug>
lute test docs/examples/games/<slug>
```
