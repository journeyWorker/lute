---
title: Lore entries
description: "The lore document kind — <entry> declarations the engine looks up instead of plays: item descriptions, found notes, inscriptions, codex pages, and barks, with state-dependent text and knowledge revealed through facts — and the <beat> blocks that bundle short scene beats beside them."
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

The top level is `<entry>` declarations and, since dsl 0.23.0, `<beat>` blocks (see
[Entries and beats in one file](#entries-and-beats-in-one-file)), and nothing else — no `# ` title
heading, no `## ` shots, no `<quest>`.

## `<entry>` attributes

| Attr | Meaning |
|---|---|
| `id` | required; unique across the project |
| `target` | the engine-owned thing it belongs to — `item.rusty_key`, `place.lab_b2`, `npc.vesna` |
| `category` | what kind of text it is — `note`, `item`, `place`, `codex`, `bark`, … (engine vocabulary) |
| `title` | display title, localized like a quest title |
| `series` / `order` | multi-part text: `order` is the position within `series` (a document-level `series:` can supply both — see below) |
| `when` | eligibility: the entry may be presented only while this holds |
| `on` / `priority` / `once` | make the entry a [beat](/language/beats/) that answers an engine occasion; `once="run"` or `once="user"` stops it answering again after a read, and on a project with a [clock](/language/clock/), `once="day"` or `once="slot"` stops it until the clock's day or slot changes |

`target` and `category` are checked for shape only, so you can write lore before the engine's item
catalog exists. Several entries may share a `target` — an NPC's barks, for example. The one
exception is an entry beat whose occasion declares a
[target domain](/language/beats/#target-domains): its `target` must then be `<prefix>.<member>` of
that domain. On an occasion declared without any target, an entry beat's `target` is metadata
(dsl 0.24.0 §6): it still says what the entry is about, and the entry answers every raise of the
occasion. Before 0.24.0 that was `E-BEAT-ATTR`, and for a scene or bundle beat it still is.

## The document is the bundle

Authors write a series the way they read it: pages one through seven, top to bottom, in one file.
A lore document can say so directly (dsl 0.19.0 §2.1):

- **`id:`** names the file as a bundle — `haven.purserLedger`, `haven.captainsLog`. It has the
  same shape rules as a scene's `id:` (`E-META-ID`), becomes the artifact's `meta.id` and the
  document's key in `project.index.json`, and shares one project-wide namespace with scene and
  quest document ids (`E-CONN-EPISODE-ID-DUP`). Quest documents may declare one too; without it a
  document is keyed by its first declared quest or entry id. A lore document that holds `<beat>`s
  must declare it, because it prefixes every beat's id.
- **`series:`** makes every entry in the file one series, ordered by **position in the file**
  (1-based). Reordering pages is moving entries; inserting a page renumbers the ones after it.

```lute check="docs/examples/haven/lore/purser-ledger.lute"
---
kind: lore
luteVersion: "0.25.0"
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
no `<on>` or `<objective>`. Anything else is `E-GRAMMAR-NOT-ADMITTED`. Story with a choice in it
belongs in a scene, or in a [`<beat>`](#entries-and-beats-in-one-file) in the same file.

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
So `when="!entry.<id>.read"` makes an entry play once per run, not once ever.

Its user-tier twin, **`entry.<id>.everRead`** (dsl 0.22.0), answers "has the player *ever* read
this?". The engine sets it whenever it sets `entry.<id>.read`, so it turns true on the first read
ever, and a new run never resets it. It is reserved the same way: readable wherever
`entry.<id>.read` is, never written by content, and resolved by `W-ENTRY-REF-UNKNOWN` like its twin.
Together the two flags tell a later run from the first one:

```lute
<entry id="tomaDejaVu" target="npc.toma" category="bark" when="entry.scientistLog1.everRead && !entry.scientistLog1.read">
  @toma: You have that look again. As if you already know what the log says.
</entry>
```

`tomaDejaVu` is eligible only in a run after the one where the player first read the log, and
only until they find it again. On its own, `when="!entry.<id>.everRead"` makes an entry play once
ever.

| Flag | Tier | Set | Reset |
|---|---|---|---|
| `entry.<id>.read` | run | after the entry's first presentation in a run | at every new run |
| `entry.<id>.everRead` | user | after the entry's first presentation ever | never |

An entry beat can also spend itself on these flags directly: `once="run"` stops it answering its
occasion while `read` is set, and `once="user"` once `everRead` is. `once="day"` and
`once="slot"` (dsl 0.24.0 §1, only with a declared [clock](/language/clock/)) spend it by time
instead: it does not answer again until the clock's day, or slot, changes, whatever its flags say.
See [Beats](/language/beats/).

### What a read proves

A guard that assumes an entry was read also knows what that entry asserted (dsl 0.24.0 §6). Under
`entry.X.read` (or `entry.X.read == true`), `lute check-project` adds the facts `X`'s body asserts
on every route, and that nothing retracts, to the facts it knows hold, the Must set of the
[fact analysis](/state/facts-and-datalog/). A `holds(…)` of one of them inside that guard is then
redundant, `W-FACT-GUARANTEED`, and its negation can never hold (`E-ARM-DEAD` on a line). Save the
first document on this page as `lore/ship-records.lute` and give `scientistLog2` a second line:

```lute
<entry id="scientistLog2" target="item.torn_note_2" category="note" series="scientistLog" order="2" when="entry.scientistLog1.read">
  @scientist: Day nine. We stopped writing her name in the logs.
  @scientist{when="holds(knows(vesna, project_lumen))"}: You know what Lumen was. Now you know who.
</entry>
```

<!-- lute-diagnostics -->
```
./lore/ship-records.lute:20:20: warning [W-FACT-GUARANTEED] guard `holds(knows(vesna, project_lumen))` is redundant: `knows(vesna, project_lumen)` is asserted on every route to here (./lore/ship-records.lute:15) (dsl 0.20.0 §5)
```

The entry's `when` already guarantees the log was read this run, so the line's guard is always
true. `entry.X.everRead` promises less. It adds only the asserted facts of `tier: user` and
`tier: app` relations, the ones a new run keeps. `knows` above is `tier: run`, so under
`everRead` alone, as in `tomaDejaVu`, its `holds(…)` stays an open question.

## Entries and beats in one file

An interview, a short NPC moment, the remark someone makes when you pick up the log: these are
scene beats, not entries, because the player plays them rather than reads them. They are also a few
lines each. Rather than one scene file per moment, a lore document can hold them as `<beat>` blocks
beside its entries (dsl 0.23.0):

```lute check
---
kind: lore
id: shipRecords
title: Ship's records
---

<entry id="bridgeLog" target="item.bridge_log" category="note" title="Bridge log">
  @narrator: The heading was changed eleven years ago.
</entry>

<beat id="tomaAtTheLog" on="examine" target="item.bridge_log" title="Toma at the log" when="entry.bridgeLog.read">
  @toma: You found it too.
  <branch id="heading">
    <choice id="tell" label="Tell her what it says">
      @toma: Then we are not going home.
    </choice>
    <choice id="hide" label="Say it is nothing">
      @toma: Your hands say otherwise.
    </choice>
  </branch>
</beat>
```

The two kinds of block share a file but keep their own rules:

| | `<entry>` | `<beat>` |
|---|---|---|
| Id | its own `id`, unique across the project | `<document id>.<beat id>`: `shipRecords.tomaAtTheLog` |
| Body | content lines, `<match>`, `::set` / `::assert` / `::retract` | a scene body: lines, branches, hubs, `<match>`, directives |
| Reached | looked up by the engine, or as an [entry beat](/language/beats/#entry-beats) | only as a beat answering its `on` occasion, once its `when` and (dsl 0.25.0) its `after=` hold |
| Effects | on the first read in a run | on every presentation, as a scene's |
| Spent | by `entry.<id>.read` / `everRead` when `once=` asks (by the clock for `once="day"` / `"slot"`) | by presentation; `once` defaults to `run` |
| Shared spend (dsl 0.25.0) | `share=` beside `once=`: reading it spends every beat of the key | `share=` beside `once`: presenting it spends every beat of the key |
| Read by conditions | `entry.<id>.read`, `entry.<id>.everRead` | `visited('<document id>.<beat id>')` |

Entries and beats may come in any order. In the compiled lore artifact each one heads its own
addressing unit, in source order: an `entry` or `beat` record, then its body, which runs to the
next `entry` or `beat` record. Beat bodies get lineIds under the canonical id
(`shipRecords.tomaAtTheLog.toma_0010`). The [Beats](/language/beats/#beat-bundles) page covers
the `<beat>` attributes, the canonical id, and how a bundle beat is checked and selected.

## Tooling

- `lute trace <doc> --entry <id> --mock m.yaml` previews one entry against mocked state; seed
  `entry.<id>.read: true`, or `entriesRead: { run: [<id>] }`, to preview a re-read.
  `entriesRead: { user: [<id>] }` seeds `entry.<id>.everRead` instead, for a document that reads
  it. `lute run <artifact> --entry <id>` does the same over a compiled lore artifact.
- `lute trace <doc> --beat <id> --mock m.yaml` previews one `<beat>` by its local or canonical id,
  with the mock's `choose:` picking its branches; `lute run <artifact> --beat <id>` does the same
  over a compiled lore artifact. The beat's `when` and `after=` are shown, not enforced. An id that names no beat
  is `E-TRACE-BEAT` in `trace` and a usage error in `run`; both list the document's beats. See
  [Tracing](/tooling/tracing/#bundle-beats).
- `lute test` tests a lore document too (dsl 0.22.0). The test names the entries to present, with
  `entry: <id>` or `entries: [ids]`. They are presented in order with the read flags set between
  them, so a repeated id is a re-read. With the first document on this page saved as
  `lore/ship-records.lute`:

  ```yaml
  file: ../lore/ship-records.lute
  entries: [scientistLog1, scientistLog2]
  expect:
    transcriptContains: ["Day nine. We stopped writing her name in the logs."]
  ```

  A lore test that names neither is `E-TEST-LORE`, and the message lists the document's entry ids.
  Since lore is testable, `lute test --coverage` lists an untested lore document with the other
  untested documents.
- `lute lore <dir>` prints the world-narrative map: entries by target and by series, with each
  entry's and beat's `when` under it, and which facts entries reveal versus scenes and quests. It
  reads beat bodies too. When the project's rules derive a relation, a **Derived** section lists
  every conclusion the rules can reach from what the project asserts, the rule instance behind it,
  the evidence it rests on (which entry or scene asserted each premise), and the entries and beats it gates
  (`derived` in `--json`). See [the CLI reference](/tooling/cli/).
- `lute new lore <name>` scaffolds a lore document.

The engine contract — eligibility, presentation, first-read effects, the two read flags — is in
[`docs/runtime/lore-entries.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md);
the normative specs are
[`0.19.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md),
[`0.22.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)
for `everRead` and lore tests,
[`0.23.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)
for beats in a lore document, and
[`0.24.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)
§6 for entry targets as metadata and what a read proves.
