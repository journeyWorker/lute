# Starfall

The story layer of a live-service gacha RPG: seven weeks of a content calendar, festivals that open and rerun, and nine heroes who each have their own summon line, birthday and bond stories.

**Genre:** live-service gacha RPG (story layer only; the engine owns the server day, summons, currencies, battles and shops, Lute owns which lines play when).

## The game

An account never starts over: everything the player keeps is `user.*`, and the clock runs over the server day `user.day` (day 1 is launch Monday). Each festival is a season whose window opening resets its event currency, its missions and its once-per-window lines.

| Days | Content |
|---|---|
| 1–7 | launch week, standard banner |
| 8–14 | Harvest Moon Festival (limited SSR Cyra); stall open to day 17 |
| 15 | Chapter 3 released |
| 22–28 | Starfall Night (limited SSR Vesper); stall open to day 31 |
| 36–42 | Harvest Moon rerun, plus an encore scene for veterans |
| 43–49 | Chapter 4 and the Frostlight Vigil (limited SSR Nyx) |

- 18 scenes: a prologue and nine main-story stages over four chapters, eight event-story stages (Harvest Moon 1–3 and the encore, Starfall Night 1–2, Frostlight Vigil 1–2).
- 15 lore bundles: one file per hero (summon intro, duplicate line, home tap, birthday, three bond stories), plus login stamps, daily and weekly resets, festival openings and last calls, Pim's event stalls, roster-wide summon and home-screen lines, and a codex.
- 9 heroes (4 SSR, 3 of them limited; 5 SR) and 3 staff speakers; 9 quests (four main chapters, three festival mission sets, a weekly tower climb, a login streak).
- No ending: the account runs on; the plays cover the first seven weeks.

## What it shows

- **Seasons** — `seasons: { harvest, starfall, frost }` over `live:` defs, `season.<name>.*` state, `tier="season:<name>"` quests, `once="season:<name>"` beats, so a rerun is one more window on the same def: `schema/calendar.schema.yaml`, `schema/world.schema.yaml`, `quests/events.lute`, `lore/resets.lute`.
- **The calendar as defs** — a parameterized def `onDays(first, last)` and named windows (`@harvestLive`, `@frostShopOpen`, `@harvestDaysLeft`, `@stageReleased`): `schema/calendar.schema.yaml`.
- **Day clock** — `raise: { dayStart: dailyReset, dayEnd: dayClose }` with a labelled `week:`; `once: day` / `once: week` / `once: slot` / `once: user` login and reset lines: `schema/world.schema.yaml`, `lore/login.lute`, `lore/resets.lute`, `lore/home.lute`.
- **Chapters** — the main story is one `chapters:` chain on `stageClear`, so a new stage is one line; stages exist only once released (`raisedWhen: "@stageReleased"`): `lute.project.yaml`, `plugins/starfall.engine/occasions/starfall.yaml`.
- **Roster at scale** — `hero` built from rarity sub-kinds (`ssr`, `sr`, `limited` via `subsetOf:`), `user.bond` and `user.birthday` `per: hero`, one bond-rank rule table (`bondRank(H, r1..r3)`), derived `celebrate(H)`: `schema/roster.schema.yaml`.
- **Kind-target and per-member beats** — `target="kind:ssr"` / `kind:sr` / `kind:limited` summon lines, `kind:hero` home taps, `for="kind:hero"` birthday and frost-candle beats once per matching hero: `lore/summons.lute`, `lore/home.lute`, `lore/resets.lute`.
- **Payloads** — `summon` carries `{ copies, streak, banner }`; duplicate and pity lines read `occasion.payload.*`: `plugins/starfall.engine/occasions/starfall.yaml`, `lore/summons.lute`, `lore/bonds/*.lute`.
- **Templates** — `bondStory` (a beat template: `on: bondStory`, `once: user`, `after: "@prev"`, `when: holds(bondRank(@who, @rank))`), `eventOpens` (`once: "season:@season"`), `loginStamp` (an effects component with a speaker param defaulting to `lumi`): `components/`, `lore/bonds/aria.lute`, `lore/resets.lute`, `lore/login.lute`.
- **`spentBy` beside a stated `once:`** — Pim's vigil nag is spent once three vigil nights are kept, and `once="season:frost"` re-arms it each window: `lore/resets.lute`.
- **`share=`** — the Frostlight last call is one spend shared between the login beat and the day-close beat: `lore/login.lute`, `lore/resets.lute`.
- **Plugin surface** — occasions with targets and `raisedWhen`, reward kinds, a world event, the `::purchase` bridge behind Pim's stall hubs, `::bondReward` / `::openBanner` / `::stamp` directives, cast files: `plugins/starfall.engine/`, `lore/shop.lute`.
- **Quests** — `rearm=` (login streak, weekly tower), `by=` deadlines on season windows, rewards, `follows=`: `quests/`.
- **Entries** — codex pages gated by progress, a locked-page fallback on `kind:topic`: `lore/codex.lute`.
- **Tests and plays** — 20 scenario tests (`tests/`) and 5 plays (`plays/`); `season-one` includes one steps file per week (`plays/steps/`).

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/starfall-gacha
lute test docs/examples/games/starfall-gacha                  # 20 tests + 5 plays
lute play docs/examples/games/starfall-gacha --script docs/examples/games/starfall-gacha/plays/season-one.play.yaml
lute play docs/examples/games/starfall-gacha --script docs/examples/games/starfall-gacha/plays/frost-vigil.play.yaml
lute beats docs/examples/games/starfall-gacha
lute calendar docs/examples/games/starfall-gacha --axis clock=1..49 --occasion dailyReset
```

## Origin

Written as a dogfood project for Lute 0.26 by a live-ops gacha designer agent persona and carried through 0.27 and 0.28; it stress-tested whether a live-ops content calendar (dates, resets, reruns) and per-character content at roster scale can be written without copy-paste.
