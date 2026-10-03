# Seven Days in Marrow Bay

A visual novel on a weekly clock: a week at an aunt's lantern shop by the sea, a lantern to build by Friday, two people to get to know, and the last bus home on Sunday night.

**Genre:** day-clock visual novel (two affection routes on a morning / afternoon / night schedule; the engine owns the clock, Lute owns what plays).

## The game

Sol arrives in Marrow Bay on Monday morning. Aunt Pell's hands shake too much to make the shop's lantern for Friday's Lantern Walk, so Sol builds it: reed for the frame from Theo at the pier (mornings), paper from Ines at the library (afternoons). The engine moves the clock, three slots a day; the player walks the town map, and who is where depends on the slot and the weekday. Theo asks Sol to the lighthouse on Wednesday night; missing it brings news at Thursday's breakfast. Ines expects Sol on Wednesday afternoon; missing it means she hands the paper over late, hurt. At the post office Sol fixes Mira's franking machine and sends postcards home, one a day. Every night Sol's diary lists who was seen.

- 12 scenes: the arrival, the Friday festival, three library scenes (`meet` → `wednesday` / `late`), two pier scenes, the lighthouse date, the post office, the Saturday market, and two routines (`shop.morning` every `dayStart`, `shop.evening` every night before Friday).
- 15 beats and 15 entries in six lore bundles (the diary, Saturday packing and the three endings, the notice board, place barks, the post office, breakfast news).
- 4 quests: the lantern (frame and paper by Friday afternoon), the lighthouse date (by Wednesday night), Ines's Wednesday appointment (starts when you meet her), three postcards (by the last post, Saturday afternoon).
- It runs 7 days, 21 slots. It ends on Sunday night at the bus shelter: Ines's or Theo's ending if Sol walked the festival with them and their affection is 4 or more, otherwise Pell's. Each ending sets `run.departed`, the schema's `terminal:`.

## What it shows

- **Engine-owned clock with a week** — `run.day` / `run.slot` with `owner: engine`, `slots:`, `raise: { slot: slotStart, dayStart: dayStart, dayEnd: dayEnd }`, `week:` with labels, `days: 7`, `raiseAtStart: true` (the engine raises `dayStart` at Monday morning, which presents `day1.arrival`; step 1 of `plays/ines-route.play.yaml` asserts it): `world.schema.yaml`.
- **Deadlines as clock positions** — defs over `clock.index` (`@wedAfternoonOver`, `@friAfternoonOver`, `@lastPostGone`), `clock.weekday` / `clock.weekdayLabel`, a parameterised def `@at(d, s)`: `world.schema.yaml`, `quests/festival.lute`, `scenes/library-ines-wednesday.lute`.
- **A derived relation as the timetable** — `present(person, place)` with `derive: true`, one rule per person guarded by `cel(...)`, read as `holds('present', ['...'])` by every place beat: `world.schema.yaml`, `scenes/`, `lore/places.lute`.
- **Plugin occasions** — `placeVisit` and `noticeBoard` with `target: { prefix: place, entity: place }` (the board limited to `members: [square]`), `select: first`, `select: all`, `select: sequence`: `plugins/sevendays.clock/occasions/clock.yaml`; a `pick:` on the board in `plays/ines-route.play.yaml`.
- **Routine and event in one slot** — priorities put the arrival over `shop.morning` and the festival over `shop.evening`; `also` beats ride along after the routine instead of replacing it (`news.theoCameBy`, `news.lanternDone`, `post.noticed`): `scenes/morning-card.lute`, `lore/shop-news.lute`, `plays/thursday-news.play.yaml`.
- **Per-member beats and `per:` state** — `run.seen` `per: person`; `for="kind:person"` writes one diary line per person seen today via `run.seen[occasion.target]`: `lore/diary.lute`.
- **Spend periods** — scenes once a run, `once: false` routines, `once="day"` entries, `once="run"` on an `also` beat: `scenes/shop-evening.lute`, `lore/post.lute`, `lore/places.lute`.
- **`spentBy`** — the franking machine is a puzzle you can fail and retry: offered at every visit until `run.machineFixed`, and the `reed` answer needs the lantern frame: `lore/post.lute`, `tests/machine-thump.test.yaml`, `tests/machine-reed.test.yaml`.
- **Templates (components)** — `postcard`, a beat template with typed params (`to: { type: { domain: recipient } }`, `rank` with a default) and a header that writes `priority: "@rank"`, `once: day`, `share: postcardDay` (one card a day across its three uses), `after`, and a `when` over `holds('sentTo', ['@to'])`, then `::body`; used by three `<beat use="postcard">` in `lore/post.lute`. `dayCard`, a component with an enum and a string param, called with `::use` per weekday: `components/`, `scenes/morning-card.lute`.
- **Quests** — accept-driven (`::accept`) and `start=` quests, `by=` deadlines, `optional` objectives, an `on="slotEnd"` objective, `<on event="questComplete">` / `questFailed` reactions, `tier="run"`; quest state read by `<match on="quest.festivalLantern.state">`: `quests/festival.lute`, `quests/postcards.lute`, `scenes/day5-festival.lute`.
- **A second run and `prev.run.*`** — `newRun: true` after a failed lantern; run-tier quests reset and Pell remembers last summer through `prev.run.lantern.paper`: `scenes/day1-arrival.lute`, `plays/no-lantern.play.yaml`.
- **Terminal endings** — `terminal: "run.departed"`, three Sunday-night endings side by side in one bundle ordered by priority, `::end{reason=…}`, a `user.endings` counter: `lore/farewell.lute`.
- **Facts, relations and labels** — run-tier `confided` and `sentTo` set with `::assert`, entity `labels:` rendered through `{{@to}}`: `world.schema.yaml`, `scenes/theo-lighthouse.lute`, `components/postcard.component.lute`.
- **Line-level details** — `:plural(…)` in the diary, choices with `into=` / `value=` writing enum state, line `when=`, `as=` speaker names, `<match>` range arms (`..2`, `3..`) and an `unset` arm with `test=`: `lore/diary.lute`, `scenes/shop-evening.lute`, `scenes/pier-theo-meet.lute`, `lore/places.lute`.
- **Own vocabulary** — a project `vocabulary.schema.yaml` with camelCase members (`fadeInUp`, `slideInRight`, `fadeOut`) and `exits:`.
- **Saves and play scripts** — plays that start from a save (`plays/postcards.play.yaml`, `plays/thursday-news.play.yaml`), `advance: slot` / `day` / a count, `include:` with `repeat:` (`plays/steps/stay-home.steps.yaml`), and `saves/after-monday.play.yaml`, a save for `lute calendar --script`.
- **Tests and plays** — 15 scenario tests (`tests/`) and 5 plays (`plays/`).

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/seven-days
lute test docs/examples/games/seven-days                         # 15 tests + 5 plays
lute play docs/examples/games/seven-days --script docs/examples/games/seven-days/plays/ines-route.play.yaml
lute play docs/examples/games/seven-days --script docs/examples/games/seven-days/plays/no-lantern.play.yaml
lute beats docs/examples/games/seven-days                        # every occasion's candidates in order
lute calendar docs/examples/games/seven-days --axis clock --occasion slotStart --facts present   # who is where, when
lute calendar docs/examples/games/seven-days --axis run.day=2..7 --axis run.slot=morning,afternoon,night \
  --occasion placeVisit --target place.library --script docs/examples/games/seven-days/saves/after-monday.play.yaml
lute trace docs/examples/games/seven-days/lore/diary.lute --beat person \
  --state occasion.target=ines --state run.day=3 --state run.seen.ines=3 --state run.aff.ines=2
```

## Origin

Written as a dogfood project for Lute 0.32 by a visual-novel writer persona used to planning a week in a schedule table; it stress-tested whether occasions and beats can replace that table: an engine-owned clock, missable dates with deadlines, routine versus event in one slot, and a writer's calendar view.
