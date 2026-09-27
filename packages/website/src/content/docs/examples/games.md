---
title: Example games
description: Fourteen complete games in the repository — mysteries, roguelikes, visual novels, live-ops, a gacha, an Ink port — each its own project, checked with zero warnings and run with its tests and plays in CI.
---

The repository ships fourteen complete games under
[`docs/examples/games/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/games). Each
was written as a dogfood project against an earlier Lute release by an agent playing a different
kind of writer, then migrated to the current language. Each is its own project — manifest, schemas,
scenes, lore, quests, plugins, `tests/` and `plays/` — with a README that names the features it
exercises, the file to look at for each, and the commands to run it.

CI checks every game with `lute check-project --deny-warnings` and runs its tests and plays, so the
games stay clean as the language moves. Where a game means something a warning would otherwise
question — a permanent ending, a latching `spentBy`, a role name two cast entries share — it says so
in the spelling Lute provides for that intent, never with a suppression.

Where to start: [`tea-hollin`](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/tea-hollin)
is the smallest, written the way the [getting-started](/getting-started/first-scene/) pages teach.
Coming from Ink or Yarn, read
[`ledger`](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/ledger) beside
[Coming from Ink or Yarn](/guides/coming-from-ink-yarn/).
[`monster-league`](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/monster-league)
shows a project at scale.

| Game | Genre | What it shows |
|---|---|---|
| [The Ashen Stair](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/ashen-stair) | roguelike hub (Hades-like) | the run boundary and `prev.run.*`, engine-owned state and reserved relations, reward kinds, a season, per-NPC send-offs with `for=` |
| [The Drowned Crown](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/drowned-crown) | roguelike (Hades-like) | run and user tiers, a permanent ending (`terminal: { when, persists: true }`), beat ladders on the hub visit, occasion payloads |
| [Ember Road](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/ember-road) | party RPG chapter | companion approval and faction reputation, a day clock of nights on the road, a `::check` dice bridge, battle payloads |
| [Harbor Days](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/harbor) | live-ops life sim | a finite clock with a week, a festival season, spend periods, `spentBy` beside `once:`, directive effects, per-villager birthdays |
| [Hollow Ward](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/hollow-ward) | survival horror / escape | rules as the door graph with `raisedWhen:`, blocking bridges, entries, terminal fates, a large test suite |
| [Lamplight](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/lamplight) | detective mystery / time loop | facts and stratified Datalog rules, user-tier evidence surviving each loop, a casebook `series:`, quests from sub-quests |
| [Lantern Academy](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/lantern-academy) | otome / dating sim with NG+ | route locks as rules, `raiseAtStart: true`, user-tier `cleared` facts across terms, date and exam bridges |
| [The Lighthouse Keeper's Ledger](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/ledger) | interactive fiction (Ink port) | Ink mapped to Lute: a hub with `<return>`, visit counters, `terminal:` as `-> END`; the Ink source beside it |
| [Skerry Rock](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/lighthouse-keeper) | deduction mystery (Obra Dinn-like) | stratified rules with `excludes:`, conclusions derived rather than asserted, a clock with `raiseAtStart`, an inquest chapter chain |
| [Monster League](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/monster-league) | creature-collector RPG | scale: five writers, per-area schemas, 18 occasions, 151 species as facts, a template used ~160 times, `sharedName: true` |
| [Seven Days in Marrow Bay](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/seven-days) | day-clock visual novel | deadlines as clock positions, a derived timetable relation, `newRun` and `prev.run.*`, three terminal endings |
| [Starfall](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/starfall-gacha) | live-service gacha (story layer) | three seasons with reruns, the content calendar as defs, rarity sub-kinds, summon payloads, `share=` |
| [Summer Station](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/summer-station) | visual novel / dating sim on a clock | schedules as rules, a `select: sequence` routine plus events, a storm season, sub-kinds with `per:` state |
| [Tea at Hollin Street](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/tea-hollin) | cozy mystery | the smallest: one chapter chain, a branch and a hub, a one-day clock, a quest with a deadline |

Run one from a checkout of the repository:

```sh
lute check-project --deny-warnings docs/examples/games/tea-hollin
lute test docs/examples/games/tea-hollin
lute play docs/examples/games/tea-hollin --script docs/examples/games/tea-hollin/plays/magpie.play.yaml
```
