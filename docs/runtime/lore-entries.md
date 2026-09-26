# Lore entries

A lore artifact (`kind: "lore"`, dsl 0.19.0) carries `entry` records: text the
engine **looks up** — an item description, a found note, a place inscription,
a codex page, an ambient bark — rather than content it plays in sequence. The
grounding here is `ir.rs::EntryCmd`, `ProjectIndex.entries`, and the proposals
[`0.19.0.md`](../proposals/scenario-dsl/0.19.0.md) and
[`0.22.0.md`](../proposals/scenario-dsl/0.22.0.md) (§7: `everRead`, entry
`once`).

Lute declares what an entry says, when it is eligible, and what reading it
changes. Where it lives is the engine's: which item spawns in which room, which
panel shows the codex, when an NPC barks.

## The record

```ts
type EntryCmd = {
  kind: "entry";
  addr: Addr;
  id: string;            // unique across the project
  target?: string;       // engine-owned id: "item.rusty_key", "npc.vesna"
  category?: string;     // "note" | "item" | "place" | "codex" | "bark" | … (engine vocabulary)
  title?: string;
  titleLineId?: string;
  series?: string;       // multi-part text, e.g. "scientistLog"
  order?: number;        // position within series
  when?: CelPair;        // eligibility
  body: Addr;            // first record of the body segment
  on?: string;           // dsl 0.21.0: the occasion this entry answers (a beat)
  priority?: number;     // dsl 0.21.0: beat priority; absent → 0
  once?: "run" | "user"; // dsl 0.22.0: beat repetition policy; absent → repeatable
};
```

The body segment follows the record and runs to the next `entry` or `beat`
record (a lore artifact interleaves entries and bundle beats, dsl 0.23.0 §4;
`beats-and-occasions.md`) or the end of the artifact — addressed and
terminated exactly as an `<on>` body is (`quest-lifecycle.md`). It contains
only `line`, `match`, `jump`, `set`, `assert`, and `retract` records: an entry
has no choices, no staging, and no handlers.

`ProjectIndex.entries` (`lute compile --all`) lists every entry in the project
as `{id, document, target?, category?, series?, order?}` in document order, so
an engine can build its `target → entries` and `series → entries` tables
without loading every artifact.

## Eligibility

An entry is **eligible** while its `when` is absent or evaluates true against
live state and facts (`evalSlot(when.raw, when.expr, …)`, `execution-model.md`).
The engine decides when to present an eligible entry — on inspect, on pickup,
when the player enters a place, when a codex page opens, on a bark trigger —
keyed by `target` and `category`.

When several eligible entries share a `target` and `category`, choosing among
them (first eligible, most recently unlocked, random, cooldown-based) is engine
policy. Lute's contribution is a stable tiebreak: `ProjectIndex.entries` order.

An entry that names an occasion (`on`, dsl 0.21.0) is also a **beat**: when
the engine raises that occasion, choosing among the eligible beats follows
[beats-and-occasions.md](./beats-and-occasions.md) — priority, then
`ProjectIndex.beats` order — and presenting the winner follows the rules
below. An entry beat's `once` (dsl 0.22.0 §7) adds to its eligibility there:
`"run"` makes it ineligible while `entry.<id>.read` is true, `"user"` once
`entry.<id>.everRead` is.

## Presentation

Presenting an entry runs its body segment from `body` in address order against
live state, like any other command stream: `line` records present, `match`
selects an arm, `jump` continues.

### First-read effects

`set`, `assert`, and `retract` records apply **only while
`entry.<id>.read` is `false`** — the entry's first presentation in a run. After
that presentation completes, the engine sets `entry.<id>.read = true` and
`entry.<id>.everRead = true`.

```ts
function present(entry: EntryCmd, state, facts) {
  const firstRead = !state.get(`entry.${entry.id}.read`);
  runSegment(entry.body, state, facts, {
    applyEffects: firstRead, // skip set/assert/retract records otherwise
  });
  if (firstRead) {
    state.set(`entry.${entry.id}.read`, true);      // run tier
    state.set(`entry.${entry.id}.everRead`, true);  // user tier
  }
}
```

Re-reading a note shows its current text — a `match` may pick a different arm
now — and changes nothing else. An author who wants a repeatable effect writes
it in a scene or quest.

## `entry.<id>.read` and `entry.<id>.everRead`

Two reserved, engine-written `bool`s per entry, both default `false`, both
readable from any CEL slot in any document kind (`<match
on="entry.scientistLog1.read">`, a quest `done=`, another entry's `when`).
Content never writes either (`E-QUEST-RESERVED-WRITE`).

| Flag | Tier | Set | Reset |
| ---- | ---- | --- | ----- |
| `entry.<id>.read` | run | after the entry's first presentation in a run | when a run starts |
| `entry.<id>.everRead` (dsl 0.22.0 §7) | user | with `read`, so after the first presentation ever | never |

`read` resets with the **run** tier (`state-lifecycle.md`): the next run's
first read applies the effects again. Before 0.22.0 an engine that kept a
codex across runs kept that record itself; `everRead` names it in the
language, so content can tell a later run from the first
(`entry.X.everRead && !entry.X.read`).

The artifact's `state` table lists `entry.<id>.read` (with `entry:<id>`
provenance) but not `everRead`. Keep an `everRead` flag for every entry you
load, `false` until its first read; a CEL slot reads it as an ordinary path,
in raw text and in the `expr` AST (`{ "path": "entry.<id>.everRead" }`).

## Knowledge

Reading an entry reveals knowledge with ordinary facts: a body's
`assert` records add them to the fact store (`cel-and-facts.md`) on first read.
Scenes and quests react with `holds(…)` guards. Nothing about knowledge is
entry-specific at runtime.

## Reference tooling

- `lute trace <doc.lute> --entry <id> [--mock m.yaml]` presents one entry
  against mocked state. Seeding `entry.<id>.read: true` (or `entriesRead:
  { run: [<id>] }`) in the mock shows a re-read: the text, without the
  effects. `entriesRead: { user: [<id>] }` seeds `entry.<id>.everRead`.
- `lute run <artifact.json> --entry <id>` does the same over a compiled
  artifact; `lute run` on a lore artifact without `--entry` is a usage error.
- `lute test` presents a lore test's `entry: <id>` / `entries: [ids]` in
  order, with the engine's flag writes applied between them, so a repeated id
  is a re-read. An entry may be named by its `<document id>.<entry id>`
  alias there, in `expect.eligible` keys, and in a play script's `pick:`,
  `entriesRead:` and step `winner` / `offered` / `notOffered` / `presented`
  (dsl 0.26.0 §8).
- An entry whose `when` is false under the test's mocks is one the engine
  would never present (dsl 0.26.0 §7): walking it fails the test unless the
  test asserts `expect: { eligible: … }` for it. A test that asserts
  `eligible:` shows the verdict on the entry's head and does not walk the body
  of an ineligible entry — it needs no bridge answer and plays no line of it.
  The same holds for a bundle beat, and a test of a scene beat (`on:`)
  may assert `eligible:` for the scene itself (its `when`, `after:` and `once:
  user` under the mocks); a scene reached by explicit flow has no such gate.
- `lute play` keeps both flags across a playthrough: a `newRun` step resets
  every `entry.<id>.read` and no `everRead`, and a script's top-level
  `entriesRead: { run, user }` starts from a save.
- `check` warns `W-ENTRY-WRITE-REREAD` (dsl 0.26.0 §8) on an entry beat
  (`on=`) without `once` whose body has a `set` or `retract` (an `assert` is exempt: the fact holds for the rest of the run anyway): every
  raise may present it again in the run, but its writes apply on the first
  read only. A write meant to repeat belongs in a `<beat once="false">`.
- `lute lore <dir>` prints the project's world-narrative map: entries and
  beats by target (a bundle beat under its `<document id>.<beat id>`, a scene
  beat under its scene key, each labelled `beat`), entries by series, and
  which ground facts entries, bundle beats, and scenes and quests reveal.
