# The Lighthouse Keeper's Ledger

A short Ink story ported to Lute: a night alone in a lighthouse, one lamp room, and the keeper's ledger that explains where he went.

**Genre:** interactive fiction, a single-location loop with three endings (a port of an Ink original).

## The game

You land on the rock at dusk and climb to the lamp room. From there you keep returning to the same menu — read the ledger (its text changes on each read), search the desk, look out from the gallery, fetch oil from the stores, say the keeper's name — until you wait for dark.

- 4 scenes: `arrival` and `keeper` in a chapter chain, then two ending scenes.
- One hub (`lamp`) holds the whole middle of the story, about 10 picks on the longest route.
- Three endings: the lamp catches (enough oil and the keeper's name), timber on rock (anything less), or you drown climbing the gallery rail after watching the water three times.
- The Ink source it was ported from is kept in `source/ledger.ink` (10 knots), so the two can be read side by side.

If you write Ink or Yarn, read this with the [Coming from Ink or Yarn](../../../../packages/website/src/content/docs/guides/coming-from-ink-yarn.md) guide ([site](https://lute-lang.vercel.app/guides/coming-from-ink-yarn/)); its "A knot, ported" section is this lamp room.

## What it shows

How Ink constructs map to Lute, each once:

- **Ink `VAR` → schema state** — `run.oil`, `run.knowsName`, `run.fate`: `world.schema.yaml`.
- **Ink `-> END` → schema `terminal:`** — `terminal: "run.fate == 'drowned'"` ends the run mid-scene; `::end` alone is Ink's `-> DONE`: `world.schema.yaml`, `scenes/keeper.lute`.
- **A knot you return to → `<hub>`** — every sub-knot is a hub `<choice>`; `*` choices are `once`, `+` choices are plain, the exit is `exit`: `scenes/keeper.lute`.
- **First-visit / return text** — text before the hub plays once; `<return>` plays on every return to the menu: `scenes/keeper.lute`, `tests/return.test.yaml`.
- **Visit counts** — scene-local counters (`scene.ledgerReads`, `scene.gallery`) declared in the scene's own `state:` and read by `<match>` and `when=`: `scenes/keeper.lute`.
- **Inline conditional text** — `@narrator{when="run.oil > 0"}: …`: `scenes/keeper.lute`.
- **Diverts to endings → chapters and `after:`** — `chapters:` orders `arrival` → `keeper`; each ending is a separate scene with `on: chapter`, `after: 'visited("keeper")'` and an exclusive `when:`: `lute.project.yaml`, `scenes/ending/`.
- **Tests and plays** — 4 scenario tests and 3 plays, one per ending (`plays/lit`, `plays/dark`, `plays/drowned`); the drowned play expects `end: terminal`.

## Run it

```sh
lute check-project --deny-warnings docs/examples/games/ledger
lute test docs/examples/games/ledger                           # 4 tests + 3 plays
lute play docs/examples/games/ledger --script docs/examples/games/ledger/plays/lit.play.yaml
lute trace docs/examples/games/ledger/scenes/keeper.lute --choose lamp=ledger,desk,name,dusk
```

## Origin

Written as a dogfood project for Lute 0.27 by an experienced Ink/Yarn Spinner writer persona porting their own Ink story; it stress-tested what Ink habits (single-brace interpolation, visit counts, diverts, `-> END`, inline comments and tags) do in a `.lute` file, which became the Coming from Ink or Yarn guide and its checker hints.
