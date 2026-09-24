# State lifecycle

The artifact's `state: StateEntry[]` (`ir.rs::StateEntry`) is the engine's
**init/type table** — the resolved, folded state schema for one document. Each
entry is:

| field        | meaning |
| ------------ | ------- |
| `path`       | the dotted state path, e.g. `run.metMira`, `scene.choices.sofaHelp`. |
| `type`       | a value-level type label: `bool` / `number` / `string` / `enum` / `narrativeTime` / `list<…>` / `map<…>` / `record`. **An AUTHOR `state:` declaration is scalar-only** — `bool`/`number`/`string`/`enum` (dsl 0.8.0 §4, `E-STATE-COLLECTION`); `narrativeTime` and the collection labels appear only on engine-surfaced slots (a reserved quest slot, or a plugin `state_shapes` expansion). |
| `domain`     | for an `enum`, its member set. An implicit branch slot or a `quest.<id>.state` slot appends `"unset"` to the domain. Absent for non-enums. |
| `default`    | the initial value (any JSON scalar/array/object, integral-collapsed). **Absent** when the slot has no default — the slot is *maybe-unset* until written. |
| `provenance` | `"branch:<id>"` for an implicit `<branch>`/`<hub>` choice slot, `"quest:<id>"` for a reserved quest slot, `"entry:<id>"` for a lore entry's reserved `entry.<id>.read` flag; absent for an author-declared slot. |

The engine initializes each declared path from `default` where present, and
treats a slot with **no `default` as unset** until the first write. Reading an
unset path is an engine-defined error/`unset` sentinel; the checker's
definite-assignment pass (`crates/lute-check/src/defassign.rs`) already proves
that no *guaranteed* read precedes a write for the monotonic tiers, so a clean
artifact never reads a provably-unset path — but a *maybe-unset* read can still
occur down a conditional path and is the engine's to define.

## Namespaces (state tiers)

The leading path segment selects one of five lifetime tiers
(`crates/lute-check/src/meta.rs::Namespace`, dsl §9.1):

| prefix        | tier    | intent |
| ------------- | ------- | ------ |
| `scene.*`     | `Scene` | per-scene scratch — choice records (`scene.choices.<id>`), hub visits (`scene.visited.<hub>.<id>`), and author `scene.*` state. |
| `run.*`       | `Run`   | per-playthrough state that persists across scenes within one run. |
| `user.*`      | `User`  | per-user/profile state that persists across runs (e.g. `user.xp`). |
| `app.*`       | `App`   | install-/app-wide state, shared across users where the host allows. |
| `quest.<id>.*`| `Quest` | scratch scoped to **one quest instance** (dsl 0.2.0 §5). May carry engine-reserved implicit sub-namespaces — `quest.<id>.state`, `quest.<id>.objectives.<oid>.done` (§5.2). |

The tier names are the contract; the DSL fixes their **relative** lifetimes and
the invariants below. The precise host events that begin a "scene" or end a
"run" (a save-load, a chapter break, a new-game) are host policy — the DSL does
not name them, and this document does not invent them.

## Initialization boundaries and reset

What the DSL *does* pin, and the engine must honor:

- **Monotonic tiers — `run.*` and `user.*`.** The connectivity envelope
  algebra assumes writes to these tiers are monotonic — *"once set, stays
  set"* — because only a full run/profile reset clears them, well outside one
  run's traversal (connectivity design spec §4.3, and
  `crates/lute-check/src/envelope.rs::in_envelope_scope`, which scopes exactly
  the `run.*`/`user.*` tiers). An engine that
  cleared `run.*`/`user.*` mid-run would violate the reachability guarantees
  `check-project` proved. These are the two tiers whose reads the
  `E-STATE-MAYBE-UNAVAILABLE` / envelope analysis reasons about.

- **`scene.*` resets at the scene boundary.** Scene scratch — including the
  implicit choice/visit records — is local to the scene that declared it.

- **`quest.<id>.*` is instance-scoped and MAY be cleared.** For a repeatable
  quest, the engine MAY clear a quest's scratch fields when it re-instantiates
  the quest mid-run (dsl 0.2.0 §5.1). This is exactly why the envelope algebra
  deliberately excludes `quest.<id>.*` from its "once set, stays set"
  reasoning (connectivity design spec §4.3) — the engine owns the clearing
  point, and no static analysis models it.

- **`app.*`** is the widest tier; its persistence and sharing are host-defined.

## Reserved quest slots

Two families of `quest.<id>.*` paths are **engine-owned**, not author-written
(the author never assigns `quest.<id>.state`, dsl §5.4):

- `quest.<id>.state` — the fixed lifecycle enum `active` / `complete` /
  `failed` / `unset`. Its `domain` in the state table appends `"unset"`, and
  its entry carries no `default`, but the slot is **always assigned**: an
  engine MUST read a quest it has not activated as `"unset"` (IR addendum
  §3.1) — never as a missing value. The checker relies on this (since lute
  `0.21.1`): a read needs no guard, `== 'unset'` is legal and means "not yet
  activated", and `isSet(quest.<id>.state)` — always true — is
  `W-QUEST-STATE-ISSET`. The engine *derives* every transition (see
  [quest-lifecycle.md](./quest-lifecycle.md)).
- `quest.<id>.objectives.<oid>.done` — a plain `bool`, recorded when the
  objective's `done` predicate first holds (monotonic within an instance).
- `quest.<id>.activatedAt` — a `narrativeTime` (dsl 0.8.0 §5), populated by the
  engine at the `unset → active` transition. This is the anchor `validAt(rel,
  t)` was missing. `validAt` asks whether the fact was valid *at* `t`
  (`established ≤ t < invalidated`, dsl 0.3.0 §3.2), so "since activation" is
  `holds(R) && !validAt(R, quest.q1.activatedAt)`. Readable in CEL under the
  ordering-only comparison surface (`<`, `<=`, `==`, `>`, `>=`; `!=` stays
  rejected, 0.3.0 D8); never author-declarable (`E-QUEST-RESERVED-DECL`) and
  never author-writable (`E-QUEST-RESERVED-WRITE`).

A `StateEntry` for these carries `provenance: "quest:<id>"`, so the engine can
tell a reserved slot from an author's own `quest.<id>.*` scratch declaration
without pattern-matching on the path.

A quest's `tier` (dsl 0.22.0 §7, `QuestCmd.tier`) decides how long these
slots live. The default `user` tier keeps them across runs. A
`<quest tier="run">` (`tier: "run"` on its record) has them reset when a run
starts — `state` back to `unset`, every objective not done, `activatedAt`
cleared — so the quest can be taken up again in the next run (see
[quest-lifecycle.md](./quest-lifecycle.md)).

## Reserved entry flags

Each lore `<entry>` has two engine-written `bool` flags, readable from any CEL
slot in any document and never author-writable (a `::set` of any `entry.*`
path is `E-QUEST-RESERVED-WRITE`):

- `entry.<id>.read` — **run** tier. `false` until the entry is first presented
  in the run; reset with the run tier, so a new run's first presentation
  applies the entry's effects again. A lore artifact's state table carries it
  (`type: "bool"`, `default: false`, `provenance: "entry:<id>"`).
- `entry.<id>.everRead` — **user** tier (dsl 0.22.0 §7). Set on the entry's
  first presentation ever and never reset by a new run. It has no state-table
  row: the engine keeps it per user beside the read flag, `false` until then.

An entry beat with `once="run"` / `once="user"` (`EntryCmd.once`) is not
eligible once the matching flag is set; see
[lore-entries.md](./lore-entries.md) for the presentation rules.

## Engine-owned paths (`owner: engine`)

A `state:` declaration may carry `owner: engine` (dsl 0.22.0 §1.2): content
reads the path freely but never writes it — a `::set` of the path, or of a
field under it, is `E-ENGINE-OWNED-WRITE`, and any other `owner:` value is
`E-STATE-DECL`. The key is a check-time contract only. It is **not** carried
in the artifact: the path's `StateEntry` is the same `path` / `type` /
`default` as any author-declared slot of its tier, and the engine initializes
and resets it by the tier rules above. What the declaration guarantees the
engine is that no `set` record in a checked artifact targets the path, so
every write it sees there is its own. `reserved: true` relations
(`RelationEntry.reserved`) are the fact-store analogue: the engine alone
asserts and retracts their facts.

## Interpolation reads

`line.text` / choice `label` keep their verbatim `{{…}}` markers; the parallel
`placeholders` list (IR A3, `ir.rs::Placeholder`) names each referent — a
state `path`, an `@`-`ref`, or a `reserved` token (only `userName` today). The
engine substitutes these against live state at present time; the raw text is
kept so an uninterpolated fallback is always available.

The artifact carries no defs table, so a `ref` placeholder carries its def
body inlined as `expr` (a `{raw, expr}` pair like every other CEL slot, since
lute 0.21.1): the engine renders `{{@twice}}` by evaluating `expr` against live
state, exactly as it would a guard. A component `{{@param}}` never reaches the
artifact as a placeholder: a param is a compile-time constant, so each `::use`
expansion's text already contains the bound literal (`Outside: grey.`). A
param bound to a caller-side def stays a `ref` placeholder naming that def.
