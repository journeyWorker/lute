# Lore entries

A lore artifact (`kind: "lore"`, dsl 0.19.0) carries `entry` records: text the
engine **looks up** — an item description, a found note, a place inscription,
a codex page, an ambient bark — rather than content it plays in sequence. The
grounding here is `ir.rs::EntryCmd`, `ProjectIndex.entries`, and the proposal
[`0.19.0.md`](../proposals/scenario-dsl/0.19.0.md).

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
};
```

The body segment follows the record and is addressed and terminated exactly as
an `<on>` body is (`quest-lifecycle.md`). It contains only `line`, `match`,
`jump`, `set`, `assert`, and `retract` records: an entry has no choices, no
staging, and no handlers.

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
below.

## Presentation

Presenting an entry runs its body segment from `body` in address order against
live state, like any other command stream: `line` records present, `match`
selects an arm, `jump` continues.

### First-read effects

`set`, `assert`, and `retract` records apply **only while
`entry.<id>.read` is `false`** — the entry's first presentation in a run. After
that presentation completes, the engine sets `entry.<id>.read = true`.

```ts
function present(entry: EntryCmd, state, facts) {
  const firstRead = !state.get(`entry.${entry.id}.read`);
  runSegment(entry.body, state, facts, {
    applyEffects: firstRead, // skip set/assert/retract records otherwise
  });
  if (firstRead) state.set(`entry.${entry.id}.read`, true);
}
```

Re-reading a note shows its current text — a `match` may pick a different arm
now — and changes nothing else. An author who wants a repeatable effect writes
it in a scene or quest.

## `entry.<id>.read`

A reserved, engine-written `bool` per entry, default `false`, readable from any
CEL slot in any document kind (`<match on="entry.scientistLog1.read">`, a
quest `done=`, another entry's `when`). Content never writes it. It resets with
the **run** tier (`state-lifecycle.md`); an engine that keeps a codex across
runs keeps that record itself.

## Knowledge

Reading an entry reveals knowledge with ordinary facts: a body's
`assert` records add them to the fact store (`cel-and-facts.md`) on first read.
Scenes and quests react with `holds(…)` guards. Nothing about knowledge is
entry-specific at runtime.

## Reference tooling

- `lute trace <doc.lute> --entry <id> [--mock m.yaml]` presents one entry
  against mocked state. Seeding `entry.<id>.read: true` in the mock shows a
  re-read: the text, without the effects.
- `lute run <artifact.json> --entry <id>` does the same over a compiled
  artifact; `lute run` on a lore artifact without `--entry` is a usage error.
- `lute lore <dir>` prints the project's world-narrative map: entries by
  target and by series, and which ground facts entries reveal versus
  scenes and quests.
