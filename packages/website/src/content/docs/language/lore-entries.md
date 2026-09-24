---
title: Lore entries
description: "The lore document kind — <entry> declarations the engine looks up instead of plays: item descriptions, found notes, inscriptions, codex pages, and barks, with state-dependent text and knowledge revealed through facts."
---

Scenes and quests put story on a **time axis**: what happens, in what order. A lot of a game's
story lives somewhere else — in **objects and places**. The torn page in the lab, the inscription
on a door, the key whose description changes after the fire, the line an NPC mutters as you walk
past. The player finds these in any order, reads them again, and pieces the story together.

A **lore document** (`kind: lore`, dsl 0.19.0) declares that text. Lute says what an entry reads,
when it is eligible, and what reading it changes. The engine decides where it lives: which item
spawns in which room, which panel shows the codex, when an NPC barks.

## A lore document

```lute check
---
kind: lore
title: Ship's records
entities:
  crew: { members: [vesna, toma] }
  topic: { members: [project_lumen, true_heading] }
relations:
  knows: { args: [crew, topic], tier: run }
state:
  run.labBurned: { type: bool, default: false }
---

<entry id="scientistLog1" target="item.torn_note_1" category="note" series="scientistLog" order="1" title="Research log, day 3">
  @scientist: Day three. Subject E does not respond to light.
  ::assert{knows(vesna, project_lumen)}
</entry>

<entry id="scientistLog2" target="item.torn_note_2" category="note" series="scientistLog" order="2" when="entry.scientistLog1.read">
  @scientist: Day nine. We stopped writing her name in the logs.
</entry>

<entry id="rustyKey" target="item.rusty_key" category="item">
  <match on="run.labBurned">
    <when is="true">
      @narrator: A scorched key. Someone's name has melted into the grip.
    </when>
    <otherwise>
      @narrator: A rusty key, stamped "Research wing B2".
    </otherwise>
  </match>
</entry>
```

The top level is one or more `<entry>` declarations and nothing else — no `# ` title heading, no
`## ` shots, no `<quest>`.

## `<entry>` attributes

| Attr | Meaning |
|---|---|
| `id` | required; unique across the project |
| `target` | the engine-owned thing it belongs to — `item.rusty_key`, `place.lab_b2`, `npc.vesna` |
| `category` | what kind of text it is — `note`, `item`, `place`, `codex`, `bark`, … (engine vocabulary) |
| `title` | display title, localized like a quest title |
| `series` / `order` | multi-part text: `order` is the position within `series` (a document-level `series:` can supply both — see below) |
| `when` | eligibility: the entry may be presented only while this holds |

`target` and `category` are checked for shape only, so you can write lore before the engine's item
catalog exists. Several entries may share a `target` — an NPC's barks, for example.

## The document is the bundle

Authors write a series the way they read it: pages one through seven, top to bottom, in one file.
A lore document can say so directly (dsl 0.19.0 §2.1):

- **`id:`** names the file as a bundle — `haven.purserLedger`, `haven.captainsLog`. It has the
  same shape rules as a scene's `id:` (`E-META-ID`), becomes the artifact's `meta.id` and the
  document's key in `project.index.json`, and shares one project-wide namespace with scene and
  quest document ids (`E-CONN-EPISODE-ID-DUP`). Quest documents may declare one too; without it a
  document is keyed by its first declared quest or entry id.
- **`series:`** makes every entry in the file one series, ordered by **position in the file**
  (1-based). Reordering pages is moving entries; inserting a page renumbers the ones after it.

```lute check="docs/examples/haven/lore/purser-ledger.lute"
---
kind: lore
luteVersion: "0.21.1"
id: haven.purserLedger
title: Purser's ledger
series: purserLedger
uses: ../world.schema.yaml
---

<entry id="purserLedger1" target="item.purser_ledger" category="note" title="Page 1">
  @ottavio{code="0010"}: Manifest amended at the third bell. Two pods logged aboard that were never loaded.
  ::assert{knows(vesna, manifest)}
</entry>

<entry id="purserLedger2" target="item.purser_ledger" category="note" title="Page 2" when="entry.purserLedger1.read">
  @ottavio{code="0010"}: If anyone reads this far: the heading on the bridge was never the true one.
  ::assert{knows(vesna, true_heading)}
</entry>
```

*(From [`docs/examples/haven/lore/purser-ledger.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/haven/lore/purser-ledger.lute).)*

`purserLedger1` compiles as `order` 1 and `purserLedger2` as `order` 2 of series `purserLedger`.
The compiled `entry` records and `project.index.json` carry the resolved `series` / `order`, so
the engine reads every entry the same way whichever form you wrote.

The per-entry `series=` / `order=` attributes (the first example on this page) remain the form for
a series that spans several files. Inside a document that declares `series:`, an entry carrying its
own `series=` or `order=` is `E-ENTRY-ATTR` — the two forms never mix in one file. Two entries that
resolve to the same `(series, order)` are `E-ENTRY-SERIES-ORDER`: per document in `lute check`,
across the project in `lute check-project`.

## What an entry body may contain

Content lines, `<match>`, `::set`, `::assert`, and `::retract`. No `<branch>` or `<hub>` (reading
has no player choice), no `<timeline>` or `::` directives (the engine owns how an entry is shown),
no `<on>` or `<objective>`. Anything else is `E-GRAMMAR-NOT-ADMITTED`.

Lines are ordinary content lines. The speaker can be `@narrator` or an in-world author such as
`@scientist`, and each entry gets its own lineId / voiceKey scope, as each quest does.

## Revealing knowledge

Reading an entry reveals knowledge with the ordinary `::assert`, against relations your schema
already declares — the checker applies the usual arity, domain, and tier rules. Scenes and quests
react with `holds(…)`:

```lute
@eris{when="holds(knows(vesna, project_lumen))"}: ...So you read the log.
```

## Reading twice

The **first** time an entry is presented in a run, its `::set` / `::assert` / `::retract` apply.
Afterwards the engine sets **`entry.<id>.read`** to `true`, and later readings show the text only —
a `<match>` may pick a different arm by then, but nothing else changes.

`entry.<id>.read` is a reserved `bool` any document can read: gate the next page of a series
(`when="entry.scientistLog1.read"`), branch a scene on it, or complete a quest objective. Content
never writes it. `lute check-project` warns `W-ENTRY-REF-UNKNOWN` when no document declares the id.

The flag is **run-tier**: a new run resets it, and the next first read applies the effects again.
So `when="!entry.<id>.read"` makes an entry play once per run, not once ever. For once ever, have the
entry set a `user.*` flag and guard on that.

## Tooling

- `lute trace <doc> --entry <id> --mock m.yaml` previews one entry against mocked state; seed
  `entry.<id>.read: true` to preview a re-read. `lute run <artifact> --entry <id>` does the same
  over a compiled lore artifact.
- `lute lore <dir>` prints the world-narrative map: entries by target and by series, and which
  facts entries reveal versus scenes and quests.
- `lute new lore <name>` scaffolds a lore document.

The engine contract — eligibility, presentation, first-read effects — is in
[`docs/runtime/lore-entries.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md);
the normative spec is
[`0.19.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md).
