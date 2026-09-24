---
title: Lore entries
description: The lore document kind — <entry> declarations the engine looks up instead of plays: item descriptions, found notes, inscriptions, codex pages, and barks, with state-dependent text and knowledge revealed through facts.
---

Scenes and quests put story on a **time axis**: what happens, in what order. A lot of a game's
story lives somewhere else — in **objects and places**. The torn page in the lab, the inscription
on a door, the key whose description changes after the fire, the line an NPC mutters as you walk
past. The player finds these in any order, reads them again, and pieces the story together.

A **lore document** (`kind: lore`, dsl 0.19.0) declares that text. Lute says what an entry reads,
when it is eligible, and what reading it changes. The engine decides where it lives: which item
spawns in which room, which panel shows the codex, when an NPC barks.

## A lore document

```lute
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
| `series` / `order` | multi-part text: `order` is the position within `series` |
| `when` | eligibility: the entry may be presented only while this holds |

`target` and `category` are checked for shape only, so you can write lore before the engine's item
catalog exists. Several entries may share a `target` — an NPC's barks, for example.

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

## Tooling

- `lute trace <doc> --entry <id> --mock m.yaml` previews one entry against mocked state; seed
  `entry.<id>.read: true` to preview a re-read.
- `lute lore <dir>` prints the world-narrative map: entries by target and by series, and which
  facts entries reveal versus scenes and quests.
- `lute new lore <name>` scaffolds a lore document.

The engine contract — eligibility, presentation, first-read effects — is in
[`docs/runtime/lore-entries.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md);
the normative spec is
[`0.19.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md).
