# Lantern Academy

An otome over one ten-day autumn term at a boarding school on a lake: four love interests, routes that lock, two endings per route, and a true route that opens only once the save remembers every other one.

**Genre:** otome / dating sim visual novel with NG+ (the engine owns the clock and the minigames, Lute owns affection, locks, routes and endings).

## The game

One run is one term. Days 1–5 are the common route: the ferry, a council hearing, Mika's ankle, the midterm, festival eve. On day 6 the player floats a lantern for one open route at the Lantern Festival; days 7–9 are that route's date, a storm and a confession; day 10 is the closing ceremony and the ending.

- Four love interests (Ren, Mika, Soren, Kai). Siding with Ren at the hearing locks Kai and the other way round; telling the coach locks Mika; failing the midterm locks Soren; a route under 3 affection is closed by rule.
- Two endings per route (lantern and ember), a lone ending when every route is closed, and the keeper's ending with Hotaru, the lantern keeper, which opens only after all four routes were cleared in earlier terms.
- The save remembers across terms: cleared endings and unlocked CGs (user-tier relations), the term count, and the last term's route and ending through `prev.run.*`.
- 17 scenes, 13 lore bundles (hangouts, dorm, barks, gallery, title, term, letters, gifts, five ending files), 4 quests, 4 components; 10 endings.

## What it shows

- **Facts and rules for route locks** — `locked(suitor)` (run tier), `routeOpen(S) :- suitor(S), not locked(S), cel("run.aff[S] >= 3")`, derived `onRoute` with `excludes: [locked]`, user-tier `cleared(suitor, outcome)` and `cgSeen(cg)`: `world.schema.yaml`.
- **Per-member state** — `run.aff` `per: suitor`, indexed as `run.aff[S]` in rules and `run.aff[@who]` in components: `world.schema.yaml`, `components/hangout-beat.component.lute`.
- **Clock with a week and `raiseAtStart`** — three slots a day, week labels, `days: 10`; the engine raises `slotStart` where each advance stops and once where a term starts, stated with `raiseAtStart: true`, so the day-1 morning arrival plays: `world.schema.yaml`, `scenes/common/arrival.lute`.
- **Scenes chained by `after:`** — each calendar scene answers `slotStart` on its own day and waits for the one before (`after: 'visited("common.exam")'`); route scenes wait for the festival: `scenes/common/`, `scenes/<suitor>/`.
- **NG+ through `prev.run.*`** — the term recap and festival eve read the last term's route and ending through enum `labels:`: `lore/term.lute`, `scenes/common/festival-eve.lute`.
- **Templates** — `afterSchool` and `shutOut` beat templates (a `beat:` header answering `visit` at `place.@place` with `once: day` and a lock guard) used by every hangout: `components/hangout-beat.component.lute`, `components/shut-beat.component.lute`, `lore/hangouts.lute`.
- **Effects components** — `cg` (`::unlockCg` plus `::assert{cgSeen(..)}`) and `ending` (sets `run.ending`, counts `user.terms`, asserts `cleared`): `components/cg.component.lute`, `components/ending.component.lute`, `lore/endings/`.
- **Plugin bridges** — `::date{with spot resultKey}` answers `mood` / `hearts`, `::exam{subject resultKey}` answers `grade` / `score`, read back with `<match on="scene.exam.midterm.grade">`: `plugins/academy.engine/directives/minigames.yaml`, `scenes/common/exam.lute`, `scenes/*/date.lute`.
- **Occasions** — `visit` raised only after school (`raisedWhen`), `gift` with `payload.hearts`, `gallery` with `select: all`, `termEnd` with `judge: before` so the ending reads judged quests: `plugins/academy.engine/occasions/academy.yaml`, `lore/gifts.lute`, `lore/gallery.lute`.
- **Per-member beats** — `for="kind:suitor"` slips a note under the door on day 8 from each suitor the player did not choose who is still warm (3+ affection) and unlocked: `lore/letters.lute`.
- **Quests** — a `by=` deadline on `clock.index` (`festivalPrep`, `studyGroup`), a weekly `rearm="clock.weekday == 0"` (`lampDuty`), a user-tier `fourLanterns` that outlives terms: `quests/term.lute`, `quests/memory.lute`.
- **Spend periods and formatting** — `once="week"` Sunday night with `clock.weekdayLabel`, `:plural` placeholders: `lore/dorm.lute`, `scenes/common/storm.lute`.
- **Own vocabulary** — a project `vocabulary.schema.yaml` with camelCase members (`fadeInUp`, `fadeOut`, `whiteOut`).
- **Tests and plays** — 19 scenario tests (`tests/`) and 7 plays (`plays/`, sharing 8 steps files); `five-terms` plays one save through five terms to the keeper's ending.

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/lantern-academy
lute test docs/examples/games/lantern-academy                 # 19 tests + 7 plays
lute play docs/examples/games/lantern-academy --script docs/examples/games/lantern-academy/plays/ren-lantern.play.yaml
lute play docs/examples/games/lantern-academy --script docs/examples/games/lantern-academy/plays/five-terms.play.yaml
lute calendar docs/examples/games/lantern-academy --axis clock=1..10 --occasion slotStart
lute scenario docs/examples/games/lantern-academy reach --endings=termEnd   # 10 endings, all reachable
```

## Origin

Written as a dogfood project for Lute 0.26 by an otome writer agent persona and carried through 0.27 and 0.28; it stress-tested route locks, per-route endings and NG+ memory across runs.
