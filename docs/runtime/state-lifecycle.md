# State lifecycle

The artifact's `state: StateEntry[]` (`ir.rs::StateEntry`) is the engine's
**init/type table** — the resolved, folded state schema for one document. Each
entry is:

| field        | meaning |
| ------------ | ------- |
| `path`       | the dotted state path, e.g. `run.metMira`, `scene.choices.sofaHelp`. |
| `type`       | a value-level type label: `bool` / `number` / `string` / `enum` / `list<…>` / `map<…>` / `record`. |
| `domain`     | for an `enum`, its member set. An implicit branch slot or a `quest.<id>.state` slot appends `"unset"` to the domain. Absent for non-enums. |
| `default`    | the initial value (any JSON scalar/array/object, integral-collapsed). **Absent** when the slot has no default — the slot is *maybe-unset* until written. |
| `provenance` | `"branch:<id>"` for an implicit `<branch>`/`<hub>` choice slot, `"quest:<id>"` for a reserved quest slot; absent for an author-declared slot. |

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
  `failed` / `unset`. Its `domain` in the state table appends `"unset"`, but —
  unlike a branch slot — it carries **no forced default**: the engine populates
  it (maybe-unset) before the quest is known (IR addendum §3.1). The engine
  *derives* every transition (see [quest-lifecycle.md](./quest-lifecycle.md)).
- `quest.<id>.objectives.<oid>.done` — a plain `bool`, recorded when the
  objective's `done` predicate first holds (monotonic within an instance).

A `StateEntry` for these carries `provenance: "quest:<id>"`, so the engine can
tell a reserved slot from an author's own `quest.<id>.*` scratch declaration
without pattern-matching on the path.

## Interpolation reads

`line.text` / choice `label` keep their verbatim `{{…}}` markers; the parallel
`placeholders` list (IR A3, `ir.rs::Placeholder`) names each referent — a
state `path`, an `@`-`ref`, or a `reserved` token (only `userName` today). The
engine substitutes these against live state at present time; the raw text is
kept so an untinterpolated fallback is always available.
