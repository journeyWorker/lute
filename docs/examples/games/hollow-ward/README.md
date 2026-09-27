# Hollow Ward

One night in a closed psychiatric hospital: find your brother, keep your nerve, get out before dawn while a dead orderly walks his round.

**Genre:** survival horror / escape (free-order exploration, time pressure, failure states; the engine owns the map, the bag, the meter and the stalker, Lute owns what each moment means).

## The game

St. Agathe's Hollow Ward, closed since the 1979 fire. At eleven at night Nell Arden goes in after her brother Tobias, who went in with a camera at nine. The Orderly, Albert Crane, still walks the three o'clock round with his gurney.

- One night, seven hourly slots (eleven to five). The move past five is dawn and the clock stops.
- 15 rooms (12 on the ground floor, 3 in the isolation wing under the boiler room). Seven are open at night; the rest open by rules the engine reads before it opens a door: a keypad code from the lobby plaque, the records key, the morgue key, the lantern, power from the boiler levers, the brass key, the ward key.
- Code and order puzzles: the pharmacy keypad, the director's safe, the gate (three torn ledger pages plus the keycard), the hydrotherapy valves (two wrong turns drown you), the boiler levers (lever III first burns you), morgue drawer 13, the chapel bell (works only at three).
- The Orderly: footsteps one room out, then an encounter with hide / run / freeze / lantern / syringe; the chase is a bridge that answers `lost | hurt | taken`. Once the bell has rung at three he goes down instead.
- The isolation wing: a three-stop lift ride down, three patients to settle before three, lights that flicker.
- 27 scenes, 9 lore bundles, 4 quests (`findTobias`, `beforeDawn`, `quietWard`, `hushWing`), 2 components.
- Endings (`run.fate`): taken, broken (sanity 0), drowned, burned, lost at dawn; out alone, out together, and released (ledger burned and bell rung at three: the ward goes to sleep). Deaths carry over: the arrival scene remembers how many nights Nell has already lost.

## What it shows

- **Rules as the door graph** — `canEnter(room)` is derived by seven rules; the engine raises `enter` only where it holds and `lockedDoor` where it does not (`raisedWhen:`), so no room scene carries a gating guard. `close(room)` joins the stalker's room with the door graph for `footsteps`: `schema/world.schema.yaml`, `schema/isolation.schema.yaml`, `plugins/ward.engine/occasions/ward.yaml`.
- **Facts and relations** — run-tier relations, a `reserved:` engine-owned `holding(item)` with `changedOn: [pickup]`, `excludes:` (`taken` vs `following`), derived `restless(P) :- patient(P), not calmed(P)`, `count(calmed(_))` in a def: `schema/world.schema.yaml`, `schema/isolation.schema.yaml`.
- **Finite clock** — `raise: { slot: hourStrikes, dayEnd: dawn }`, `days: 1`, enum `labels:` for the hours, `clock.index` in defs (`@lateNight`): `schema/world.schema.yaml`, `lore/hours.lute`.
- **Terminal** — `terminal: "run.fate != 'alive'"` ends the run on any death or exit; `lute play` refuses further steps: `schema/world.schema.yaml`, `plays/released.play.yaml`.
- **Chapters** — the lift ride is a `chapters:` chain on `liftStops`, one stop per raise: `lute.project.yaml`, `scenes/isolation/descent*.lute`.
- **`spentBy`** — puzzle scenes spent by the fact they set (`holds(solved(officeSafe))`, `run.bellRung`); the freezer instead uses `once: false` plus a `when:` because the Orderly can retract `solved(freezer)`: `scenes/puzzles/`.
- **Plugin bridges and directives with effects** — `::keypad` and `::evade` (blocking bridges read back through `scene.<bridge>.<key>`), `::give` / `::consume` / `::lure`, and `::fright` whose declared write lowers the engine-owned `run.sanity`: `plugins/ward.engine/bridge/ward.yaml`, `plugins/ward.engine/directives/ward.yaml`.
- **Entries** — notes, pages and a tape as `<entry>` on `read@note.*`; the tape's `::fright{when="!entry.tape.read"}` costs sanity on the first listen only: `lore/notes.lute`, `tests/tape-first.test.yaml`, `tests/tape-reread.test.yaml`.
- **Per-member beats and payloads** — `for="kind:patient"` presents the flicker beat once per restless patient, reading `occasion.payload.seconds` with `:plural`: `lore/isolation.lute`, `plays/isolation.play.yaml`.
- **Components** — `chase` is an effects component with an enum param used by `::use` in each encounter tactic; `soothe` is a beat template (`beat:` header with `target: "fixture.@bed"` and `spentBy`) used by `<beat use="soothe">`: `components/`, `lore/orderly.lute`, `lore/isolation.lute`.
- **Quests** — `fail=` on defs, a `by=` deadline, `start=` on an entry being read, `questFailed` handlers reading `failedBy`: `quests/night.lute`, `quests/isolation.lute`.
- **Hubs and `also` beats** — the laundry search hub, a Tobias line added `also` after the isolation room: `scenes/isolation/laundry.lute`, `lore/isolation.lute`.
- **Tests and plays** — 43 scenario tests (`tests/`) and 7 plays (`plays/`), one per way the night ends plus the isolation wing.

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/hollow-ward
lute test docs/examples/games/hollow-ward                     # 43 tests + 7 plays
lute play docs/examples/games/hollow-ward --script docs/examples/games/hollow-ward/plays/released.play.yaml
lute play docs/examples/games/hollow-ward --script docs/examples/games/hollow-ward/plays/isolation.play.yaml
lute beats docs/examples/games/hollow-ward                    # every occasion's candidates in order
lute calendar docs/examples/games/hollow-ward --axis clock --axis run.bellRung=false,true --axis run.stalker=chapel --occasion encounter --target room.chapel
```

## Origin

Written as a dogfood project for Lute 0.26 by a survival-horror designer agent persona and carried through 0.27 and 0.28; it stress-tested free-order exploration with many gated rooms, time pressure and failure states without drowning content in guards.
