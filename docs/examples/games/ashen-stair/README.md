# The Ashen Stair

The Hearth side of a climb-die-return roguelike: the engine owns the tower (floors, fights, death), Lute owns what the three people at the bottom of it say about each climb.

**Genre:** roguelike hub game (Hades-like; the engine runs the climb and reports how it ended, Lute plays the hub, the bosses' falls and the endings of runs).

## The game

Wren wakes on warm stone at the Hearth, below a Stair of nine floors, with a Gatekeeper on the fourth and a Warden at the top. She climbs, falls or gets out, and wakes at the Hearth again. Before each climb she can talk to Maud at the kettle, Oskar at the cold forge and Sable, a ghost by the cistern. What they say moves on with the run count, their bond with her, what they have seen, and how the last climb ended (floor, outcome, what killed her). Oskar wants the Gatekeeper's maul-head back and reforges it into a hammer she can carry. Sable wants her feather taken out under the sky. Bonds high enough earn keepsakes and send-offs at the door. The studio can switch on the Ashfall, a live event in which ash-lilies come down the Stair for Maud.

- 19 scenes: 6 at the Hearth (`scenes/hearth/`), 10 on the Stair (at the start of a run, when a guardian falls, and at the end of a run), and 3 after the sky (`scenes/sky/`).
- 8 lore files: 15 bundle beats (the NPCs' conversations and 3 keepsakes) and 28 entries (barks, the board by the door, the departure send-offs, a 4-page codex).
- 5 quests: the long climb to the sky, Oskar's cold forge, his run challenge to carry the hammer to the top, Sable's feather, and the Ashfall lilies.
- No fixed length and no `terminal:`; runs repeat. A fresh save finishes the three main quests in four climbs (`plays/four-climbs.play.yaml`). The story ends with the sky scenes, after four escapes.

## What it shows

- **Engine-owned state and reserved relations**: `owner: engine` on `user.runs`, `run.floor`, `run.outcome`, `run.fellTo`, `user.ashfallOn`, and the `reserved: true` relations `slew`, `everSlew` and `wields`. Plays write them with `engine:` steps. See `world.schema.yaml` and `plays/`.
- **The run boundary and `prev.run.*`**: the Hearth starts a run. `prev.run.outcome`, `prev.run.floor` and `prev.run.fellTo` read the climb that just ended, and `run.fellTo` is typed `{ domain: foe }`, so `{{prev.run.fellTo}}` renders the foe's label. Plays use `newRun: true`. See `scenes/hearth/first-return.lute`, `scenes/hearth/welcome-back.lute` and `lore/maud.lute`.
- **Per-member state**: `user.bond` with `per: npc`, read as `user.bond.maud` and `user.bond[occasion.target]`. See `world.schema.yaml` and `lore/departure.lute`.
- **Plugin occasions**: targets (`npc.*`, `boss.*` typed by `foe`), `raisedWhen` (the Hearth only while `run.floor == 0`, a boss only while climbing), a typed `payload` (`runEnd { lilies }`), and `select: first` / `all` / `sequence`. See `plugins/game.occasions/occasions/stair.yaml`.
- **Reward kinds**: `EMBERS` credits `user.embers`, which the forge spends, and `KEEPSAKE` takes a typed `keepsake` target. See `plugins/game.occasions/rewardkinds/stair.yaml`, `tests/hammer-trial-top.test.yaml` and `tests/long-climb-escape.test.yaml`.
- **Plugin directives with declared effects**: `::arm` retracts `wields(_)` and asserts `wields(@weapon)`, and `::gift` asserts `gave(@from, @item)`. See `plugins/game.occasions/directives/stair.yaml`, the rack beat in `lore/oskar.lute` and `components/keepsake.component.lute`.
- **Facts, rules and derived relations**: `confides(sable) :- saw(sable, wardenFall), cel("user.bond.sable >= 2")`, `keepsakeFrom(N) :- gave(N, _)`, and `holds(...)` in guards and objectives. See `world.schema.yaml`, `tests/sable-lost.test.yaml` and `tests/sable-lost-distrust.test.yaml`.
- **A hand-written `after:` chain**: the three sky scenes answer `hubVisit` in order through `after: 'visited("sky.letter")'` and `after: 'visited("sky.kettleOutside")'`, gated on `user.escapes`, with no `chapters:` block. See `scenes/sky/`.
- **Seasons**: `seasons: ashfall: { live: "user.ashfallOn" }`, `season.ashfall.lilies`, `prev.season.ashfall.lilies` when the event returns, `once: season:ashfall`, and a `tier="season:ashfall"` quest with `fail="!user.ashfallOn"`. See `world.schema.yaml`, `scenes/hearth/ashfall-opens.lute`, `quests/ashfall.lute` and `plays/ashfall.play.yaml`.
- **Spend periods, `share` and `spentBy`**: the scenes use `once: user` / `run` / `false`, and side beats use `also: true`. `share: ashfallNews` spends Maud's Ashfall bark once the Hearth scene has told the news. Oskar's offer uses `spentBy="quest.coldForge.state != 'unset'"`. See `scenes/stair/`, `lore/maud.lute`, `lore/oskar.lute` and step 3 of `plays/ashfall.play.yaml`.
- **Per-member beats**: `for="kind:npc"` gives one send-off per NPC who gave a keepsake and trusts Wren, in member order, inside a `select: sequence` occasion. It uses `{{occasion.target}}` and `<match on="occasion.target">`. See `lore/departure.lute`, `tests/send-off-sable.test.yaml`, and the `departure` steps in `plays/four-climbs.play.yaml`.
- **Templates (components)**: `keepsake` is a beat template with typed params (`speaker`, `{ entity: keepsake }`, `number`), `effects: true`, and a header that writes `target: "npc.@who"` and `when: "user.bond[@who] >= @need"`, plus `::body`. It is used by `<beat use="keepsake">` in `lore/keepsakes.lute`. `hearthFire` takes an enum param and is called with `::use{component="hearthFire" flare=@fireFlare}`. See `components/`.
- **Quests**: `start=` predicates, an accept-driven quest (`::accept{quest="coldForge"}`), and objectives judged on an occasion (`on="bossDefeated" target="boss.gatekeeper"`, `on="runEnd"`). A run-tier quest has a `by="@fellThisClimb"` deadline, where done is judged before `by`. See `quests/`, `tests/cold-forge-wrong-boss.test.yaml` and `tests/hammer-trial-top.test.yaml`.
- **Defs**: guards (`@cameBackFromFall`, `@veteran`, `@escapedThisClimb`) and a value (`@fireFlare`, a conditional `'high' : 'low'`). See `world.schema.yaml`.
- **Lore**: bundle beats (spent when presented) versus repeatable entries, a `<hub>` with `once` and `exit` choices, a codex with `series:`, and a page gated on `entry.codexHounds.read`. See `lore/maud.lute` and `lore/codex.lute`.
- **Text hints and labels**: `:plural(one lily|# lilies)`, `{{user.runs:ordinal}}`, and entity `labels:` for foes, weapons and keepsakes. See `scenes/hearth/ashfall-opens.lute`, `scenes/hearth/veteran-welcome.lute`, `lore/hearth-board.lute` and `world.schema.yaml`.
- **Own vocabulary**: a project `vocabulary.schema.yaml` with camelCase members (`fadeInUp`, `fadeOut`, `emberBurst`, `ashFall`) and `exits:`.
- **Tests and plays**: 16 scenario tests (`tests/`) and 4 plays (`plays/`). The late-save plays start from save seeds (`presented:`, `quests:`, `entriesRead:`, `prev.run.*`).

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/ashen-stair
lute test docs/examples/games/ashen-stair                           # 16 tests + 4 plays
lute play docs/examples/games/ashen-stair --script docs/examples/games/ashen-stair/plays/four-climbs.play.yaml
lute play docs/examples/games/ashen-stair --script docs/examples/games/ashen-stair/plays/ashfall.play.yaml
lute beats docs/examples/games/ashen-stair                          # each occasion's beat ladder
lute trace docs/examples/games/ashen-stair/lore/keepsakes.lute --beat maud --state user.bond.maud=4
lute trace docs/examples/games/ashen-stair/quests/bonds.lute --state user.knowsSableName=true --state user.bond.sable=2 --state run.carriesFeather=true --state run.outcome=escaped --fact "saw(sable, wardenFall)" --occasion runEnd
```

## Origin

Written as a dogfood project for Lute 0.21 by a scenario-writer persona building a Hades-like hub game. It stress-tested engine-owned state, the run boundary (`prev.run.*`), bosses as targeted occasions and quests that span runs. Later rounds added per-member state, fact directives, beat templates, `for=` and a live-event season.
