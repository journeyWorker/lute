# The Drowned Crown

A diver dies in a sunken palace, wakes on his ship, and goes down again until he takes the crown.

## Genre

Roguelike (Hades-like): runs, a hub between runs, and a story that advances with the run count.

## The game

Ilo dives into the drowned palace of Vael from the *Gull's Mercy*. Each dive is one run: the engine
reports how deep he got, whether he died or surfaced, and which guardian (the Long Eel, the Drowned
Choir, the Archivist, the Tide Regent) fell or killed him. Between dives the crew — Captain
Brannoch, the cartographer Sefa, the cook Tavi and Quill, a ghost at the figurehead — remember what
happened and move their stories on as Ilo's bond with them grows.

- 23 scenes (the dive, the hub ladder, boss kills, Sefa's talks, the drowned library, the last
  dive), 9 lore documents of barks, bond stories and codex entries, 3 quest documents with 8
  quests (run-, user- and season-tier), 3 components.
- Open-ended length: the plays cover four dives from a fresh save, a veteran save six dives in,
  and chapter 5 from a save three dives past the Regent.
- Every dive ends drowned or surfaced (`scenes/run/`). The game ends for good when Ilo puts the
  crown on in the last dive (`scenes/last/crown.lute`); leaving it keeps the game open.

## What it shows

- **Run and user tiers.** Engine-owned `run.*` dive state that resets every run beside `user.*`
  counters, bonds and pearls that survive it; `prev.run.*` read at the next dive's start
  (`scenes/run/recap.lute`). Schema: `world.schema.yaml`.
- **A permanent ending.** `terminal: { when: "user.crowned", persists: true }` in
  `world.schema.yaml`: the ending outlives runs on purpose, so `lute play` reports the game over for
  good instead of offering a new run (`plays/library.play.yaml`).
- **Occasions plugin.** `plugins/game.occasions/`: `hubVisit`, targeted `talk` and `bossDefeated`,
  `runStart` (`select: sequence`), `runEnd` (`judge: before`, a `pearls` payload), `keepsakes`
  (`select: all`), `sluice` and `lastDive` gated by `raisedWhen`; world events, a `::salvage`
  directive that writes season state, reward kinds `PEARLS` (credits `user.pearls`) and `KEEPSAKE`.
- **Beat ladders.** The hub visit: first waking (`once: user`, p100) → Regent fell (p50) →
  died/surfaced (p10) → deck fallback, plus an `also` side remark (`scenes/hub/`). Brann's
  run-count ladder (`lore/brann.lute`), Sefa's scene ladder (`scenes/talk/`).
  `lute beats` prints them.
- **Per-member beats.** One `<beat for="kind:bonded">` gives everyone whose bond is high enough a
  word at the rail, in turn, on the `runStart` sequence (`lore/sendoff.lute`).
- **Chapters.** The last dive is a `chapters:` chain on `lastDive` (`lute.project.yaml`: rail,
  throne, crown), raised only once `@readyForCrown` holds.
- **Seasons.** The neap tide, live two dives in six (`seasons:` in `world.schema.yaml`); a
  `once="season:neap"` bark and `prev.season.*` (`lore/neap.lute`), a `tier="season:neap"` quest
  (`quests/library.lute`). `lute calendar` over `user.runs` shows it switch on.
- **Facts and rules.** Reserved `slew` (run) / `felled` (user) asserted by the engine; derived
  `heard` (gossip spreads aboard, Quill saw it) and `trusts` with a `cel(...)` premise
  (`world.schema.yaml`); `::assert{told(sefa, eel)}` in `scenes/talk/sefa-eel.lute`.
- **spentBy.** The sluice puzzle retries every dive until it is solved, then gives way to the open
  doors (`scenes/library/sluice.lute`, `scenes/library/doors.lute`).
- **Quests.** User-tier relationship quests (`quests/crew.lute`), run-tier dive quests with a
  `by=` deadline, an `on="runEnd"` objective and a range reward (`quests/dive.lute`), a quest
  accepted between dives for the next one with `::accept{… at="nextRun"}` (`lore/tavi.lute`).
- **Components.** `gauge` with a `@def` default (`components/gauge.component.lute`), `keepsake`
  with effects, and `bondStory`, a component that is itself a beat (`beat:` frontmatter), used
  five times in `lore/bonds.lute`.
- **Plays.** `engine:` steps writing run outcomes, `newRun`, `repeat:`, saves seeding
  `prev.run.*`, `presented:`, `entriesRead:` and quest states (`plays/selection.play.yaml`).

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/drowned-crown
lute test docs/examples/games/drowned-crown
lute play docs/examples/games/drowned-crown --script docs/examples/games/drowned-crown/plays/four-dives.play.yaml
lute play docs/examples/games/drowned-crown --script docs/examples/games/drowned-crown/plays/library.play.yaml
lute beats docs/examples/games/drowned-crown --occasion talk --target npc.sefa
lute calendar docs/examples/games/drowned-crown --axis user.runs=1..6 --occasion talk --target npc.tavi
```

The project also demonstrates the 0.36.0 analysis surfaces:

```sh
lute impact docs/examples/games/drowned-crown fact:felled(regent)
lute constraints docs/examples/games/drowned-crown
```

`impact` reports the transitive reverse-dependency closure with reason chains;
its `evidence` values distinguish proven links from heuristic overlap. The
constraints report lists every manifest invariant, including `unknown`
verdicts.

## Origin

Written as a dogfood project for Lute 0.23 by an agent persona (round 3) and carried to 0.29; it
stress-tested run/user tiers, `prev.run`, a hub between runs and quests that span dives.
