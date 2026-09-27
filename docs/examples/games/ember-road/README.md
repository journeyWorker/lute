# Ember Road

A caravan warden has four nights to get eleven wagons across a bridge held by the people the caravan's owners wronged.

## Genre

Party RPG chapter (Baldur's Gate / Dragon Age style): companions with approval, camp talk, banter,
faction reputation, skill checks and an engine-fought battle.

## The game

The Warden leads the Ashen Company's caravan from Ostwick along the Ember Road to the Tollbridge,
where Maelis of the Duskbound holds the chain. Isolde and Corvin travel with the Warden; Wren, a
local at Harrow's Ford, can join. Along the way: the miller's kidnapped twins, a smugglers' tunnel,
the Company's burn ledger, the ruins of Coldwater, and a camp every night.

- 12 scenes (the road, the bridge, camp), 6 lore documents (banter, camp talk, bridge talk, searches,
  Coldwater, a codex series), 3 quest documents with 8 quests (the main quest, opening the bridge
  with two approach quests, the rescue, two companion quests, Coldwater), 3 components.
- One chapter over at most four nights (`clock: days: 4`); a full route plays in 9–16 script steps.
- Routes and endings, one play each: talk Maelis off the chain with proof (`parley`), sell the
  deserters for the toll (`betrayal`), storm the bridge and win (`battle`) or lose (`defeat`), pay
  with the strongbox (`persuade`), prove it with Coldwater's roll (`coldwater`), let the fourth night
  pass (`deadline`), or lose every wagon (`ashes`, `terminal:`). Companions can fall or walk out.

## What it shows

- **Clock.** Nights on the road as a day-granular clock: `advance: day` raises `camp` (dayEnd),
  then `travel` (the slot); `days: 4` (`world.schema.yaml`). Quest deadlines read the engine's
  night counter: `by="run.night >= 3"` / `>= 4` (`quests/ember-road.lute`).
- **Terminal.** `terminal: "run.wagons <= 0"` over run state (`world.schema.yaml`,
  `plays/ashes.play.yaml`).
- **Occasions plugin.** `plugins/game.occasions/`: `travel`, `camp` (`select: sequence`), targeted
  `talk` and `explore` with `raisedWhen` (no talking to the fallen; Coldwater only from the bridge
  leg), `banter`, `battleEnd` with a `wagonsLost` payload (`scenes/bridge/victory.lute`,
  `scenes/bridge/defeat.lute`), `chapterEnd` (`judge: before`).
- **Engine directives and a bridge.** `::check` rolls dice through a declared bridge capability and
  writes `scene.check.<key>.*` the scene reads next (`plugins/game.occasions/directives/check.yaml`,
  `bridge/dice.yaml`, `state/shapes.yaml`; used in `scenes/road/tunnel.lute`); tests and plays
  answer it with `bridges:` (`tests/tunnel-persuade.test.yaml`). `::encounter` hands a fight to the
  battle system; `::pickup` asserts `knows(item)` as its declared effect.
- **Per-member state.** `run.approval` per companion and `run.rep` per faction with per-member
  defaults (`world.schema.yaml`).
- **Facts and rules.** Reserved `fell` written by the battle system; derived `inParty`, `loyal`,
  `mourned`, `hostile` (with a `cel(...)` premise); `sidedWith` `excludes: [wronged]`; cast
  `present:` from `inParty` so companions speak only while they travel with the Warden.
- **Quests.** Accept-from-dialogue (`::accept` in `scenes/road/ford.lute`, `lore/camp-talks.lute`),
  `complete="any"` alternatives and `superseded` failures (`openBridge`, `quests/ember-road.lute`),
  targeted objectives (`on="talk" target="npc.maelis"`), consolation rewards (`outcome="failed"`),
  `<on event="battleEnd">`, a threshold `start=` (`quests/companions.lute`), `questTier: run` in
  `lute.project.yaml` defaults.
- **Per-member beats.** The night watch, one `<beat for="kind:companion">` on the camp sequence
  (`lore/camp-talks.lute`).
- **spentBy.** Searches that can fail and be retried until the thing is found: the millrace, the
  twelfth crate (`lore/places.lute`), the Coldwater chapel roll (`lore/coldwater.lute`).
- **Components.** `reaction` (approval change plus the companion's toast), `joins`, and `idle`, a
  beat component instantiated per companion (`components/`).
- **Plays.** A shared opening spliced in with `- include: steps/to-the-ford.yaml`, `engine:` steps
  for battle outcomes, saves that start at the chain (`plays/defeat.play.yaml`).

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/ember-road
lute test docs/examples/games/ember-road
lute play docs/examples/games/ember-road --script docs/examples/games/ember-road/plays/parley.play.yaml
lute play docs/examples/games/ember-road --script docs/examples/games/ember-road/plays/battle.play.yaml
lute beats docs/examples/games/ember-road --occasion talk --target npc.isolde
lute calendar docs/examples/games/ember-road --axis run.leg=1..3 --occasion travel
```

## Origin

Written as a dogfood project for Lute 0.23 by an agent persona (round 3) and carried to 0.29; it
stress-tested the quest model against party-RPG branching: alternatives, accept-from-dialogue,
engine-decided outcomes and companion state.
