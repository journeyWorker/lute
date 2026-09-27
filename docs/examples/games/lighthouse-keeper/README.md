# Skerry Rock

An inspector has three days on a lighthouse rock to work out what happened to the keeper who vanished in a gale.

## Genre

Deduction mystery (Return of the Obra Dinn style): progress is what the inspector knows, and every
conclusion is derived from evidence and testimony.

## The game

Keeper Elias Voss vanished on the night of 14 October. The inspector lands on Skerry Rock on a
Thursday and has until the relief boat sails on the fourth day to read the keeper's log, search the
lantern, the gallery, the landing and the cottage, interview the islanders the engine schedules
each day (Hollis the boatman, the postmistress Maren, the fisherman Tobias, the assistant keeper
Ada), and file a reconstruction the Board of Trade then hears. The wrecked boat points at Tobias;
the truth needs a broken alibi, an honest misidentification and a torn log page.

- 13 scenes (arrival and mornings, five places, nightfall, the report, a three-scene hearing), 6
  lore documents (the log series, wreckage, interviews, the case notebook, places, the torn page),
  3 quest documents with 4 quests, 2 components.
- Four days on the clock; the full inquiry is 27 play steps.
- Endings: the true account (he fell; Ada was on the rock; Maren lied; the rail was reported and
  never mended), a wrong reconstruction the Board rejects at the hearing, or no report at all when
  the boat sails (`terminal:`).

## What it shows

- **Facts and stratified rules.** Testimony (`sawAt/4`) and evidence (`placedBy/4`, `implicates`,
  `damaged`) are base facts; `contradicts`, `liar`, `misidentified`, `alibied`, `suspect`,
  `cleared`, `onRock`, `seenAfter`, `fell`, `preventable` are derived with negation, and
  `excludes:` states which never hold together (`world.schema.yaml`). `fell(elias)` is never
  asserted: the report only offers it once the rules derive it (`scenes/report.lute`).
- **Clock with a week.** `run.day` with `raise: { slot: arrive, dayEnd: nightfall }`, week labels
  starting on Thursday, `days: 4`, and `raiseAtStart: true` because the morning boat lands the
  inspector on the first day too (`world.schema.yaml`, `scenes/arrival.lute`).
- **Chapters.** The Board's hearing is a `chapters:` chain on `inquest` (`lute.project.yaml`):
  `board.hearing`, `board.rail` (only for a neglect finding), `board.close`.
- **Terminal.** `terminal:` over run state: the file closed, or the report quest failed by its
  deadline (`world.schema.yaml`, `plays/missed-boat.play.yaml`).
- **Occasions plugin.** `plugins/skerry.occasions/`: targeted `explore` (with a `raisedWhen` that
  keeps the village boat in harbour in a gale) and `examine`, `interview` (`select: all`),
  `nightfall` (`select: sequence`), `inquest` (`judge: before`), and a `::pocket` directive whose
  declared effect asserts `carrying(item)`.
- **Engine schedule and presence.** The reserved `present/2` relation says who is where today; cast
  `present:` keeps an absent islander silent (`world.schema.yaml`).
- **Lore series and entry beats.** The keeper's log as a chained series on `examine item.logbook`
  (`lore/keeper-log.lute`), the case notebook series whose entries appear as conclusions become
  derivable (`lore/notebook.lute`), `{vo}` lines read in Ada's hand. `lute lore` maps them.
- **spentBy.** Piecing the torn page retries until the right scrap is chosen
  (`lore/torn-page.lute`).
- **Quests.** A `countDistinct(...)` objective, `on="inquest"` objectives with `until=` for a wrong
  answer, a `by="run.day > 3"` deadline, a `questFailed` handler gated on `failedBy`
  (`quests/inquiry.lute`).
- **Choices into state.** `<choice into="run.verdictFate" value="fell">` with `when` over derived
  facts (`scenes/report.lute`).
- **Components.** `witness`, a beat component used for every interview (`components/witness.component.lute`,
  `lore/witnesses.lute`); `lampLighting` with an enum param defaulting to a `@def`
  (`components/lamp-lighting.component.lute`).
- **Per-member beats.** `thoughtsSuspect`, one `<beat for="kind:person">` on the nightfall sequence
  (`lore/places.lute`).

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/lighthouse-keeper
lute test docs/examples/games/lighthouse-keeper
lute play docs/examples/games/lighthouse-keeper --script docs/examples/games/lighthouse-keeper/plays/true-account.play.yaml
lute play docs/examples/games/lighthouse-keeper --script docs/examples/games/lighthouse-keeper/plays/missed-boat.play.yaml
lute calendar docs/examples/games/lighthouse-keeper --script docs/examples/games/lighthouse-keeper/plays/true-account.play.yaml --until "the weather clears; the engine schedules Ada" --occasion nightfall --facts liar --facts alibied --facts suspect
lute calendar docs/examples/games/lighthouse-keeper --axis run.weather=gale,fog,clear --occasion explore --target place.village
lute lore docs/examples/games/lighthouse-keeper
```

## Origin

Written as a dogfood project for Lute 0.23 by an agent persona (R3Detective, round 3) and carried
to 0.29; it stress-tested stratified Datalog with negation as the whole progress model of a
deduction game.
