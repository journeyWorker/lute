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

## Guarded writes (`::set{… when}`)

`::set{run.aff.wren += 1 when="run.warmed.wren < run.day"}` (dsl 0.24.0 §1)
writes only when its `when` condition holds. It carries no IR field of its
own: the compiler lowers it to the same one-arm `match` a `when=`-gated line
becomes (subject = the guard, one `$` arm holding the `set`, an empty
`otherwise`), so an engine runs it with its ordinary `match`, and an
undecided guard is an undecided match subject. A guarded write is never a
definite assignment: a later read of a path with no `default` stays
maybe-unset unless something unconditional writes it.

Since dsl 0.26.0 §4 a directive takes the same guard: `::use`, `::accept`,
`::assert`, `::retract` and a plugin passthrough (or bridge) directive with
`when="…"` lower to the same one-arm `match` around the record — for a
`::use`, around its whole expansion, which runs entirely or not at all. A
guarded `::assert` is never a guaranteed fact and a guarded `::accept` never a
definite accept. A builtin-lowered directive (core staging, `::end`,
`::mark`, a plugin `lower:` record) has no guard: it runs where it stands.

## Bridge answers

A plugin call's `fromBridgeResult` effect writes the answer into the result
slot the directive declares (`state.declares`) — in the host too when the call
sits in a component body (dsl 0.26.0 §3.1): the `::use` declares the slot, so
the compiled `state` table types it. The reference runner types an answer by
that slot, else by the bridge capability's `result:` shape, and refuses an
answer neither types instead of storing it as a string.

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

## The clock

A schema MAY declare one clock (dsl 0.24.0 §1) over existing engine-owned
paths — it adds meaning, not storage (D-B):

```yaml
clock:
  day: run.day            # number path, owner: engine
  slot: run.slot          # optional: enum path, owner: engine
  slots: [morning, afternoon, night]   # with `slot`: the order; the slot enum's members
  raise: { slot: slotStart, dayStart: dayStart, dayEnd: dayEnd }  # optional; each key optional
  week: { length: 7, first: 1, labels: [Sun, Mon, Tue, Wed, Thu, Fri, Sat] }  # optional
```

The declaration is carried verbatim as `clock` on the artifact and on
`ProjectIndex` (absent without one; two different clocks in one project are
an index conflict). `day` and `slot` stay ordinary `StateEntry` rows,
initialized and reset by their tier; the checker requires both
`owner: engine`, a number `day`, an enum `slot` whose members are exactly
`slots`, and declared `raise` occasions (`E-CLOCK-DECL`).

**A day-granular clock** declares no `slot` / `slots`: every day is one
slot, a position is its day alone (`day 3`), `clock.index` is `day - 1`,
and `once: slot` spends like `once: day`.

**`raise`** is one occasion (`raise: slotStart`, the same as `raise: {
slot: slotStart }`) or a map of moments:

| key | raised |
| --- | ------ |
| `slot` | once after every advance, where the clock stops |
| `dayEnd` | at every midnight an advance crosses, before the crossing — at the day's last slot, the day not yet advanced |
| `dayStart` | at every midnight an advance crosses, after it — at the next day's first slot |

**Reserved read-only paths.** The engine derives these from the live `day`
and `slot` — they are not rows of the state table, content never writes
them (`E-QUEST-RESERVED-WRITE`), and they are unset while `day` is not a
whole number or `slot` not one of `slots`:

| path | value |
| ---- | ----- |
| `clock.index` | `(day - 1) * len(slots) + index of slot in slots` (`day - 1` without slots) — monotone in time |
| `clock.weekday` | `(week.first + day - 1) mod week.length` (only with `week:`) |
| `clock.weekdayLabel` | `week.labels[clock.weekday]`, renderable (only with `week.labels`) |

**`once: day` / `once: slot`.** A scene beat (`meta.beat.once`) or entry
(`EntryCmd.once`) with `day` / `slot` is spent from its presentation until
the clock's day (for `slot`: day and slot) changes; the engine keeps the
position of the last presentation per beat. A project without a clock may
not use them (`E-BEAT-ATTR`).

**The engine moves the clock forward only.** `clock.index` never
decreases within a run; a run start resets `day` / `slot` to their
defaults by the tier rule. An advance moves the clock and settles the quest
lifecycles — a `by` deadline over the clock fails at the advance that passes
it (`quest-lifecycle.md` §Objectives). Each midnight it crosses is a stop
when the clock raises `dayEnd` or `dayStart`: the clock moves to the day's
last slot, settles, raises `dayEnd`; moves to the next day's first slot,
settles, raises `dayStart`. Then it moves to where the advance ends, applies
any other engine writes of the same moment (so a day's `dayEnd` still reads
the day it closes), settles, and raises `slot` — once, never at the slots
it passed.

The reference tooling models exactly this. A `lute play` step `advance:
slot` (one slot), `advance: <n>` (`n` slots) or `advance: day` (the first
slot of the next day, from any slot) writes the paths — wrapping past the
last slot into the next day — settles, then raises `raise.slot` with the
step's `pick:` / `choose:` and selection `expect:` (refused when the clock
raises no `slot` occasion). `advance: <n>` never skips a day's close — it
walks to each day's last slot to raise `dayEnd`; `advance: day` raises
`dayEnd` where the clock stands. The transcript prints each midnight raise
under the step (`── step 4 · day 1 (Mon) night · dayEnd`), and `--json`
lists them under `advance.days`. An `advance:` step's `engine:` writes land
where the clock arrives, after its midnight stops. Its `expect.presented`
lists what every raise of the step presented (each `dayEnd` / `dayStart`,
then the `slot` raise), in order; `winner`, `offered` and `notOffered`
judge the `slot` raise. An `occasion:` step that raises the clock's own
`dayEnd` / `dayStart` prints a note — the next advance across that
midnight raises it again. An `engine:` step that moves
`clock.index` backward is a usage error (exit 2); `newRun` starts the clock
over. `lute calendar --axis clock=d1..d2` expands to every slot of those
days in clock order (bare `clock`: one week from day 1, or day 1 without a
`week:`); `--occasion dayEnd@clock.day,clock.slot=night` (or the clock's own
paths, `@run.day,run.slot=night`) evaluates an occasion once per day.

`advance: { to: night }` and `advance: { to: { weekday: Fri, slot:
morning } }` (dsl 0.26.0 §7) move to the next position after the current
one with that slot and/or weekday (a `week.labels` label or a
`clock.weekday` number) — forward only, and never zero steps: already there,
the clock moves on to the next such position (tomorrow night, next Friday
morning), as every advance moves it. A slot or weekday the clock does not
declare is a usage error (exit 2). A step's `expect: { clock: { weekday,
slot, day } }` (any of the three) judges where the clock stands after the
step, so an `include:`d steps file states the time it expects and fails at
its first step when an earlier file moved the clock elsewhere.

## Previous run (`prev.run.*`)

`prev.run.<path>` (dsl 0.23.0 §6) is a reserved, read-only mirror of every
declared `run.<path>`: the value that path held when the previous run
ended. The engine snapshots it at run end — copy every `run.*` value to
`prev.run.*` (a path unset at run end is unset in the mirror), **then**
reset the run tier. Before the first run ends every `prev.run.*` read is
`unset`, so the checker treats the mirror as maybe-unset (a read needs
`isSet(prev.run.x)` or an `unset` arm, `E-MAYBE-UNSET`), types it as the
`run.*` path it mirrors, and rejects a content write
(`E-QUEST-RESERVED-WRITE`). The mirror is **not** in the artifact's state
table — it is implied by the `run.*` entries. `lute play` snapshots at
`newRun` and a play script's `state:` may seed `prev.run.*` (a save made
after a run ended); a `lute trace` / `lute test` mock may seed it too.

The snapshot MUST be atomic: all mirrors are written together, and a `run.*`
path with a `default` always holds a value at run end. The checker relies on
this (dsl 0.24.0): once any `prev.run.<p>` is known present (`isSet`, an arm
narrowing it, a beat `when:`), every `prev.run.<q>` whose `run.<q>` has a
`default` is treated as present too.

## Rewards that credit state

A `rewardKinds:` entry may declare `credits: <state path>` (dsl 0.23.0
§8). The compiler stamps that path onto each reward of the kind
(`RewardEntry.credits`), and a grant adds the reward's `amount` to it — the
engine owns the write, like a `grant` itself. `lute run` / `lute play` do the
same for a scalar amount and record it on the `grant` record as
`credited: { path, value }`; a range amount is the engine's roll (0.16.0
D-C) and is not credited by the reference runner. Content that also
`::set`s the path in the same quest's `<on>` or objective body pays twice
(`W-REWARD-DOUBLE-CREDIT`).

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
A `speaker` param (dsl 0.24.0 §4) is likewise spliced as the bound cast
member's display `name` (its id when it has none) in text and in an
attribute value (`as=@who`, dsl 0.26.0 §3.1), so the artifact carries plain
text; a `@@who:` line (dsl 0.26.0 §3.2) is emitted with the bound member id
as its `speaker`. A `{{…}}` placeholder inside a string argument keeps its
`placeholders` record. The writes of an `effects: true` component are ordinary `set` /
`assert` / `retract` commands in the host's stream, inside the component's
`source { component }` region; the IR has no separate record for them.

An interpolation MAY carry a format hint (dsl 0.24.0 §4):
`{{user.deaths:ordinal}}`. Its `path` or `ref` placeholder then carries
`format: "ordinal"` (absent when the author wrote none; the marker in `text`
keeps the hint), and the engine renders the value in that format — a
locale-aware engine may render its locale's ordinal. The reference runner
(`lute run`, `lute play`) and `lute trace` render English ordinals: a
non-negative integer `n` gets the suffix of its last digit — `1st`, `2nd`,
`3rd`, `4th` … `0th` — except that last two digits `11`–`13` take `th`
(`11th`, `12th`, `13th`, `111th`, `112th`; `21st`, `22nd`, `101st`). Any
other number (a fraction, a negative number) renders unchanged, as it
would without the hint.

`{{run.day:ordinalWord}}` (dsl 0.25.0 §8) carries `format: "ordinalWord"`:
an ordinal word, which the engine localizes. The reference runner and
`lute trace` render the English words `first` … `twentieth` for 1–20 and
fall back to the `ordinal` digits otherwise (`0th`, `21st`); a number with
no ordinal renders unchanged.

The checker admits `ordinal` and `ordinalWord` only on a number-typed
referent (`E-REF-TYPE` otherwise) and no other hint (`E-CEL-PROFILE`). A
component `{{@n:ordinal}}` bound to a literal number is rendered at compile
time (`3rd`; `third` with `:ordinalWord`), since no placeholder survives the
splice.

A state path typed against a named enum (`run.wd: { type: { domain: weekday } }`)
whose declaration carries `labels:` (`enums: { weekday: { members: [mon, sun],
labels: { sun: Sunday } } }`, dsl 0.24.0 §1) has those labels on its state
entry (`state[].labels`, member → display text). A `path` placeholder renders
`labels[value]` when the current value has a label and the value itself
otherwise, so `Today is {{run.wd}}.` reads `Today is Sunday.` for `sun` and
`Today is mon.` for `mon`. A `prev.run.*` read renders with its `run.*`
path's labels. `lute run`, `lute play`, `lute trace` and `lute test` all render
this way.
