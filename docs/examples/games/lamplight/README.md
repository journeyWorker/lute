# Lamplight

A whodunit on a night train that loops. Inspector Wren has until the Lamplight Express reaches Ossery at six to work out who killed Aurel Hask in compartment 7. Nearly everything she learns is a fact, and Datalog rules draw the conclusions.

**Genre:** detective mystery / time loop (a knowledge-driven investigation; the engine owns the train and the hour, Lute owns what plays).

## The game

The porter knocks at 2:31 and Hask is found dead. From then on the engine raises the moments: the inspector walks the cars, examines clues, interviews passengers, reads her casebook, and the corridor clock strikes the hour. The stopped watch points at the singer, Lise Varo, but that is a false lead. The lamp knock fixes the time of death in the Orla tunnel, the testimony gives three of the four suspects an alibi, and Dr. Solt's empty berth gives him away. At four the train stands at Harl junction long enough for one telegram. At six she must name somebody. A wrong name, silence, or Solt without proof fails the case file. The night loops, and what she learned carries into the next one.

- 5 scenes: the opening knock (`lamplight.discovery`) and its second-night version (`lamplight.again`), the dining-car hub, the Harl telegraph office, and the accusation at Ossery.
- 16 beats and 25 lore entries across six bundles: the clues (`examine`), interviews with 10 testimony entries and 2 beats (`talk`), a 12-page casebook, a 3-page ledger, the hourly timetable, and a fallback for walking the cars.
- 7 quests: the case file (`lamplightCase`) with four sub-quests (time of death, motive, the tunnel alibis, the accusation), Lise's letter, and the Harl telegram.
- One run is one night: five hourly slots from `h02` to `h06`. Any answer at Ossery, silence included, ends the run (schema `terminal:`). The accusation succeeds only if she names Solt once `prime(solt)` holds (motive, means, and no alibi for the tunnel). With the other lines of inquiry done, the case file then completes and credits a `CASE` reward. Every other outcome fails the case file, and the next run opens with `lamplight.again`.

## What it shows

- **Facts and Datalog rules** — 20 relations (3 seeded, 9 found or told, 8 derived). The 8 rules use stratified negation (`not alibi(P, S)`), `S != D`, and the `suspect` kind as a predicate. `count('opportunity', [_])` defs are rendered with `{{@unaccounted:plural(one suspect|# suspects)}}`: `world.schema.yaml`, `lore/casebook.lute`, `tests/accusation-alibi.test.yaml`.
- **Tiers across a loop** — evidence relations are `tier: user` and survive a new run, while the case quests are `tier="run"` and reopen. `entry.X.everRead` is used where `.read` would reset, and `prev.run.accused` picks last night's line: `world.schema.yaml`, `quests/case.lute`, `scenes/discovery-again.lute`, `plays/loop.play.yaml`.
- **Finite clock** — `slots:`, `raise: { slot: hourStrikes }`, `last: { day: 1, slot: h06 }`, and `terminal: "run.accused != 'nobody'"`: `world.schema.yaml`.
- **Plugin occasions** — targets (`npc.*`, `obj.*`, `place.*`), `select: first` / `all` / `sequence`, and `raisedWhen` that opens Harl only at `h04` and the accusation only at `h06`: `plugins/lamplight.occasions/occasions/train.yaml`.
- **Plugin directive and reward kind** — `::evidence{clue=…}` has a typed `entity: clue` attr and `effects.asserts: ["found(@clue)"]`, and `CASE` credits `user.casesClosed`: `plugins/lamplight.occasions/directives/evidence.yaml`, `plugins/lamplight.occasions/rewardkinds/case.yaml`.
- **Beat bundles and kind beats** — `scenes/examine.lute` holds the clue beats; `watchAgain` takes over from `watch` once `falseClue(watch)` holds, and a `target="kind:clue"` fallback speaks `{{occasion.target}}` through entity `labels:`. See also `lore/cars.lute`.
- **Per-member beats** — `for="kind:suspect"` over a `subsetOf: person` sub-kind: at five, Crane counts every suspect who still has opportunity, and anyone whose berth stood empty gets a line (`rollCall`, `rollBerth`): `lore/timetable.lute`, `world.schema.yaml`, step 18 of `plays/solve.play.yaml`.
- **Lore entries and series** — testimony entries marked `once="run"` fill a `select: all` interview menu. The casebook is a `series:` of pages gated on derived facts, and the ledger pages chain through `entry.ledger1.read`: `lore/talk.lute`, `lore/casebook.lute`, `lore/ledger.lute`.
- **Spend rules** — `spentBy: "run.wired != 'none'"` closes the telegraph office once a wire is sent. Beside it: `once: user` on the first knock, `once: false` on the dining car, and a `once` hub choice guarded by a fact: `scenes/harl.lute`, `scenes/discovery.lute`, `scenes/dining-car.lute`.
- **Templates (components)** — `lampsOut` takes `string` and enum params, dispatches with `<match subject="@depth">` and interpolates `{{@where}}`. `wire` is a beat template whose header writes `on`/`once`/`title`/`when` from params (`run.wired == '@to'`) and places the host's `::body`; it is used by `<beat use="wire">`: `components/`, `lore/timetable.lute`.
- **Branches and matches** — a `<branch>` whose choices write `into="run.accused"`, a `<match>` with an `is="solt" test="holds('prime', ['solt'])"` arm before the bare `is="solt"` arm, and an `::accept{quest=…}` inside a choice: `scenes/accusation.lute`, `lore/talk.lute`.
- **Quests** — a parent built from `quest=` sub-quests, `start=`, an `optional` objective, `on="accuse" until=…`, a `by="clock.index >= 3"` deadline, `<reward>`, and `questComplete`/`questFailed` handlers: `quests/case.lute`, `quests/harl.lute`.
- **Own vocabulary** — a project `vocabulary.schema.yaml` with camelCase members (`fadeInUp`, `fadeOut`, `blackOut`).
- **Tests and plays** — 21 scenario tests (`tests/`), covering scene, bundle-beat, `entries:` sequence, `eligible:`, and quest tests. There are 7 plays (`plays/`): the full solve, a two-night loop with `newRun` and per-step `choose:`, a second night started from a save (`visited`, `presented`, `entriesRead`, `prev.run.accused`), and four accusation outcomes.

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/lamplight
lute test docs/examples/games/lamplight                           # 21 tests + 7 plays
lute play docs/examples/games/lamplight --script docs/examples/games/lamplight/plays/solve.play.yaml
lute play docs/examples/games/lamplight --script docs/examples/games/lamplight/plays/loop.play.yaml
lute beats docs/examples/games/lamplight --occasion hourStrikes   # the hourly ladder, per-member beats included
lute calendar docs/examples/games/lamplight --axis clock --occasion hourStrikes
lute calendar docs/examples/games/lamplight --script docs/examples/games/lamplight/plays/solve.play.yaml --until "five o'clock, Crane counts heads" --axis run.hour=h05 --axis 'holds(seen(crane, brakevan, tunnel))=false,true' --occasion hourStrikes --facts opportunity
lute trace docs/examples/games/lamplight/scenes/accusation.lute --project docs/examples/games/lamplight --state run.hour=h06 --fact 'alive(hask, late)' --fact 'unanswered(cab7, tunnel)' --fact 'motive(solt)' --fact 'means(solt)' --choose accuse=solt
lute scenario docs/examples/games/lamplight knowledge              # every fact-guarded condition back to its producers
```

## Origin

Written as a dogfood project for Lute 0.21 by a detective-mystery writer persona. It stress-tested Datalog deduction with negation, knowledge that survives a looping night while the case file resets, lore series and testimony entries under test, and plays that start from a save.
