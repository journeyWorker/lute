# Changelog

All notable changes to the Lute **toolchain** are documented here. The format
is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Lute tracks three independent version axes; this file covers only the first:

- **Toolchain** — this changelog. The version of the CLI, checker, compiler,
  LSP, and npm launcher that ship together, stamped from the Cargo workspace
  (`CARGO_PKG_VERSION`) and printed by `lute version`.
- **Language** — currently `0.24.0`, the grammar and semantics the checker
  enforces. Its history lives in the versioned spec stack under
  [`docs/proposals/scenario-dsl/`](docs/proposals/scenario-dsl), not here.
- **IR** — the compiled JSON artifact schema, stamped as `irVersion` in every
  artifact (currently `0.24.0`) and gated on by consuming engines.


Every release holds all three axes **aligned** at one visible number, so a
release presents one number and nobody has to reconcile three. Alignment is a
presentation guarantee, not a claim that every axis changed substantively — this
changelog is where you learn which ones did. `0.10.1` was the no-op case, both
language and IR; `0.10.2` inverted it — the IR earned the move and the language
did not. `0.11.0` is a third shape: the **toolchain** is the one that earns it
this time (a new scheduling layer, a new command, and a bug class closed in the
shared reference runner), while both **language** and **IR** are content
no-ops — language `0.11.0` is byte-for-byte `0.10.2` semantics
([`0.11.0.md`](docs/proposals/scenario-dsl/0.11.0.md)), and the IR carries no
shape or content change at all. The one cost this release does not get to
skip: the IR's `major.minor` still moves, `0.10` → `0.11`, purely because
alignment re-aligns every visible number on every release — so an engine gated
on IR `0.10` **must widen its gate to `0.11`** even though there is nothing new
to read once it does, and `schemas/lute-ir-0.10.schema.json` is renamed to
[`schemas/lute-ir-0.11.schema.json`](schemas/lute-ir-0.11.schema.json) (body
unchanged) under the same precedent `0.7.0` set for a minor move with no shape
change.
See [`docs/versioning.md`](docs/versioning.md) for the full policy and the axes
table.

## [Unreleased]

### Added

- `{{run.day:ordinalWord}}`: an ordinal-word format hint beside `:ordinal`
  (dsl 0.25.0 §8). The IR placeholder carries `format: "ordinalWord"` for the
  engine to localize; `lute play` / `lute run` / `lute trace` render the
  English words `first` … `twentieth` and fall back to `:ordinal` digits
  (`21st`) above; the checker admits it wherever `:ordinal` is admitted.
- **Shared spends** (dsl 0.25.0 §2, SU N9): a scene (`share:`), bundle
  `<beat share=…>` or `<entry share=…>` beside a written `once` names a
  project-wide key; presenting any beat of the key (an entry: reading it)
  spends every beat of it for the `once` period. `E-BEAT-ATTR` for a
  malformed key, `share` without a written spending `once` (absent or
  `false`), and (`check-project`) beats of one key with different `once`s.
  `lute play` / `lute calendar` spend the key together (`✗ … once: day —
  share: solWarm already spent today by talks.solWarmRadio`), `lute beats`
  shows the key in its `once` column (`share` in `--json`), the presence
  ladder and `W-BEAT-PRIORITY-TIE` read the whole key's flags. IR:
  `BeatIr.share`, `EntryCmd.share`, `BeatCmd.share`, `IndexBeat.share`
  (absent when unauthored).
- **Bundle beat `after=`** (dsl 0.25.0 §3, LH N9): `<beat after="…">` has a
  scene `after:`'s meaning — an eligibility conjunct (`lute play`,
  `lute calendar`, `lute trace --beat`: `not eligible: \`after\` prerequisite
  not satisfied`) and a scenario edge (`E-CONN-PROFILE`,
  `E-CONN-UNKNOWN-NODE`, cycles, envelopes as for a scene; `scenario reach`
  prints the `after:` formula and its referenced nodes). IR:
  `BeatCmd.after` plus a `prereqEdges` row keyed by the beat's canonical id.
  `lute scenario` lists a scene or bundle beat whose `when` has a
  `visited()` conjunct but no `after` as unanchored, with the `after` to
  write (`unanchoredHints` in `--format json`).
- **Exclusive relations** (dsl 0.25.0 §1, LH F17): a relation may declare
  `excludes: [other, …]` — the relations it never holds together with on the
  same arguments. The declaration is symmetric, and each partner must be a
  declared relation with the same argument kinds (else `E-RELATION-DECL`).
  `check-project` reads `holds(A(x)) && holds(B(x))` as false (a dead guard,
  `E-ARM-DEAD` / `E-BEAT-UNREACHABLE` naming the exclusion) and
  `!holds(B(x))` as following from `holds(A(x))` — in the same guard, an
  enclosing one, or anything else on every route — so such a guard is
  `W-FACT-GUARANTEED`, and cast presence uses it too. A guard's
  `holds(dead(x))` counts inside its region even when `dead` is
  engine-`reserved` (crown M1). An `::assert{A(x)}`
  where `B(x)` holds on every route to it is the new **`E-FACT-EXCLUSIVE`**,
  and a rule that derives `A` from `B` on the head's own arguments
  (`dark(X) :- lit(X)` with `dark` excluding `lit`) is the new
  **`E-RULE-EXCLUSIVE`**, reported at the rule (LH N17). Where both are
  only possible, `lute play` reports
  `✗ exclusive: fell(elias) and seenAfter(elias) both hold` at the write
  that made them hold — even when a later write of the same presentation
  undoes it (ER C3) — or, for an `engine:` write, at the step, and halts
  (exit 1); `lute trace` / `lute test` record the same `✗ exclusive` line at
  the write and refuse the walk there (`E-FACT-EXCLUSIVE`, exit 1). Seeded
  facts are checked before anything runs (LH N16): a trace `--fact` / mock
  or test `facts:` that already breaks an exclusion refuses the trace, and
  a play script's `facts:` halts the play before step 1. Derived relations
  are covered through their derivations. A relation in an exclusion is read
  by it — no `W-RELATION-UNREAD` (ER C4). The IR's
  `RelationEntry` carries `excludes` (the symmetric closure, sorted; absent
  when empty).
- **Presence after engine events** (dsl 0.25.0 §6): an engine-`reserved`
  relation may name the occasions on which the engine changes it —
  `fell: { args: [companion], reserved: true, changedOn: [battleEnd] }`.
  Cast `assume: true` then no longer reads it as unchanged in a unit
  presented on one of those occasions (a scene / entry / bundle beat `on`,
  a quest `<on event>` handler or `on=` objective body on it) nor, in
  `check-project`, in any after-descendant of such a unit over the scenario
  graph's `after:` / `after=` / `[start]` edges, nor under a guard (a unit's
  `when`, a choice / arm / handler / line `when`) that needs a fact of it —
  `holds(fell(isolde))`, `count(fell(_)) >= 1`, or a derived relation every
  rule of which needs one (ER C2): `W-CAST-ABSENT` reports the post-battle
  line again and says `assume: true` does not cover `fell` there. Without
  `changedOn` nothing changes. `changedOn` on a relation that
  is not `reserved: true`, or naming an undeclared occasion (with a
  did-you-mean), is `E-RELATION-DECL`.
- **Quest graph edges** (dsl 0.25.0 §4, ER F17 / N13): `lute scenario`
  draws `quest(parent) -> quest(child) [subquest]` for every nested quest,
  so a parent with children is no longer listed as unanchored, and a quest
  without `after=` is anchored by its top-level `start` conjuncts that read
  `visited('…')`, `entry.X.everRead` or `quest.Y.state == '…'` (not
  `unset`) — `[start]` edges; an `||` of such reads is one anchor with
  several sources. A `start`-driven quest no longer needs a copy of its
  `start` in `after=`. The sources join the graph (an entry `X` as the new
  node `entry(X)`); `reach` lists a quest's anchors (`anchors` in
  `--format json`). An explicit `after=` still replaces the `start` and
  `::accept` anchors; the subquest edge stays. Anchors never prove a quest
  unreachable, and one that would close a cycle is not drawn.
- **`<quest accept="external">`** (dsl 0.25.0 §5, LH N5): the quest is
  accepted outside the script (a quest board, a menu, a UI). It silences
  `W-QUEST-NEVER-ACCEPTED`; beside `start` it is `E-ATTR-TYPE`, and on a
  subquest child that activates with its parent it is `E-ACCEPT-TARGET`
  (declare `activate="accept"` too). IR: `QuestCmd.accept: "external"`.

### Changed

- Bridge answers (dsl 0.25.0 §7): a bridge result field no content reads may
  be left out of a `lute play` / `lute trace` / scenario-test / `mocks/*.yaml`
  `bridges:` answer, and every unanswered-call hint lists only the fields
  content reads. A field content reads is still required
  (`E-TRACE-MOCK-TYPE`). `lute play` no longer halts at a plugin call none of
  whose result slots content reads.
- `W-QUEST-NEVER-ACCEPTED` (dsl 0.25.0 §5, LH N5): a `*.test.yaml` or
  `mocks/*.yaml` `accepts:` mock no longer counts as an acceptance — a mock
  proves a test, not the game. A quest only a mock accepts still warns, the
  message names the mock file, and the hint offers `accept="external"`.
- `lute scenario knowledge` (dsl 0.25.0 §9, LH N6): a defeater lists every
  derivation route of the defeating fact (`⇐ … / ⇐ …`), not only the first.
- The `lute scenario` note on undrawn references now says a quest's edges
  come from its `after`, subquest tree, `start` conjuncts and `::accept`s.
  A quest's `visited()` read outside its top-level `start` conjuncts (in
  `fail`, or an objective's `done` / `when` / `by` / `until`) is noted as
  `reads visited('…') in its objective watch done — a condition read, not an
  anchor` and no longer suggests declaring `after`: copying such a read into
  `after=` replaced the quest's real anchor (its `::accept`) with a backwards
  edge (summer S1). `--format json` gives the read's `slot`.
- `W-LUTE-VERSION-STALE` for a stamp inherited from the manifest's
  `defaults: luteVersion` (LH N18): `check-project` reports it once, at the
  manifest's `luteVersion:` line, with the number of documents inheriting
  it, instead of once per document at `1:1`; a single-file `lute check`
  says the stamp comes from the manifest's `defaults:`.

### Fixed

- A `share` without a written `once` (or with `once` `false`) is reported
  once — the per-file `E-BEAT-ATTR` "without `once`" — and no longer also as
  a project-wide `E-BEAT-ATTR` claiming the beat declares a different
  `once: run` than its key (dsl 0.25.0 §2, summer S2). Such a beat joins
  no key.
- An undeclared `<match on>` subject is no longer also `E-NONEXHAUSTIVE`
  (dsl 0.25.0 §9, SU N8): its read is reported once — `E-UNDECLARED`, or,
  while a schema import is broken, the import error (a `clock:` that
  swallowed the state entry left only a misleading `E-NONEXHAUSTIVE`).
- `W-CAST-ABSENT` in `check-project` no longer counts an effects
  component's `::assert{rel(@param)}` as a producer of every member: a
  component's writes produce facts only at its `::use` sites, with the bound
  arguments (ER C1). An unused `joins` component no longer made a
  character's pre-recruitment lines warn.

## [0.24.0] - 2026-09-25

**Clocks, quest structure, parties.**

A minor release from a third dogfood round over four games — a mystery, a
roguelike, a day-clock visual novel and a party RPG. A schema may declare a
clock that beats, `lute play` and `lute calendar` read; quests gain accepted
subquests, alternatives, place-bound deadlines and a reason they failed;
entities gain sub-kinds and per-member state; a cast may say when a speaker
is present; components may write state; and plugin calls are answered by
`bridges:` in play, test and trace. Every silent wrong answer that round found
is fixed here (there is no separate 0.23.2). The language and the IR both earn
the move (the IR additively); see
[`docs/proposals/scenario-dsl/0.24.0.md`](docs/proposals/scenario-dsl/0.24.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Added

- **`::clear`** (dsl 0.24.0 §4, T3-17): a core staging leaf, no attributes —
  every character on stage exits (those on stage on only some path to it
  too, as at a `::bg`), while the background and music stay. It compiles to
  one `sprite` `exit` record per character (provenance `by: stage-clear`),
  no record of its own and no new IR kind. A later line by a cleared
  character without a re-show is `W-STAGE-ABSENT` naming the `::clear`;
  `lute play` prints `::clear` where it ran and `lute trace` records it as
  an exit.
- **Interpolation format hint `{{x:ordinal}}`** (dsl 0.24.0 §4, T3-18):
  `{{user.deaths:ordinal}}` renders `1st` `2nd` `3rd` `4th` … `11th` `12th`
  `13th` … `21st` `22nd` … `101st` `111th`. The IR placeholder gains
  `format: "ordinal"` (omitted without a hint, so existing artifacts are
  byte-identical); `lute run`, `lute play`, `lute trace` and `lute test`
  render English ordinals, and a number with no ordinal (a fraction, a
  negative) renders unchanged. `ordinal` is the only hint (another is
  `E-CEL-PROFILE`), and it needs a number (`E-REF-TYPE` on a string, enum
  or bool path/def, or on `userName`).
- **`_` in rule bodies, and `countDistinct`** (dsl 0.24 T3-9). A `_` in a
  rule body atom is a fresh anonymous variable (`testified(W) :- seen(W, _)`);
  under `not` it is existential (`not seen(W, _)`: no such tuple at all). It
  stays an error in a rule head and in a comparison. The new CEL function
  `countDistinct(<pattern>, <Var>)` counts the distinct values of the pattern
  position `<Var>` names — `countDistinct(sawAt(W, _, _, _), W) >= 3` counts
  witnesses, where `count(sawAt(_, _, _, _))` counts tuples. It is decided by
  `check-project`'s fact envelope, evaluated by trace/test/play, and, like
  `count`, forbidden in a rule guard.
- **Entity sub-kinds and entity-indexed state** (dsl 0.24.0 §3, T2-6/T2-7).
  `entities: { companion: { subsetOf: person, members: [isolde, corvin] } }`
  declares a sub-kind: every member must also be a member of the parent
  (else `E-ENTITY-KIND-SHAPE` naming the outsiders; an undeclared or `open:`
  parent, an `open:` sub-kind or a `subsetOf:` loop is the same code), it is
  legal wherever a kind is, and sharing members with its parent (or a sibling
  sub-kind) is no longer `E-ENTITY-KIND-CLASH`. A state path declared
  `{ type: number, default: 0, per: companion }` declares `run.approval.<m>`
  for every member `m` of a closed kind declared in the same document (an
  `open:`, unknown or malformed kind is `E-STATE-DECL`). A rule `cel()`
  guard may read `run.approval[P]` for a variable `P` bound by a positive
  atom that ranges it over the index kind or a sub-kind (`E-FACT-DOMAIN`
  otherwise; unbound is `E-DATALOG-UNSAFE`, a non-indexed family
  `E-UNDECLARED`); anywhere else a member is named, and reading the family
  says so. Such a rule is compiled **grounded** — one IR rule per member,
  `raw` suffixed `[P = isolde]` — so IR guards stay CEL over ground terms and
  `lute trace`/`play`/`run` evaluate the same instances.
- **Cast presence and per-speaker emotions** (dsl 0.24.0 §4, T2-8/T3-17). A
  cast entry — a plugin `cast/*.yaml` export or a schema's `cast:` — may
  declare `present: "<condition>"` and `emotions: [...]`
  (`isolde: { name: Isolde, present: "holds(inParty(isolde))", emotions:
  [calm, fierce] }`). In a schema, a `present:` that does not parse is
  `E-CEL-PARSE` and one outside the CEL profile `E-CEL-PROFILE`, and an
  `emotions:` member outside the schema's own `emotion` enum is
  `E-BAD-ENUM`, each at the entry's key. A plugin cast without the new keys
  keeps its `capabilityVersion` stamp.
- **`W-CAST-ABSENT`** (new warning, dsl 0.24.0 §4): a line by a speaker
  whose cast declares `present:`, where the guards around the line do not
  imply it — its own `when=`, enclosing `<choice when>` (branch and hub),
  `<match>` arms (the arm's `is=`/`test`, and that no earlier arm matched),
  `<on when>`, `<objective done>`, and the scene beat's `when:` or the
  entry's / bundle beat's `when=`, `@def`s expanded. A guard stops counting
  once a `::set`, `::assert`/`::retract` or `::accept` that may change it
  runs between it and the line. `check-project` also counts facts that hold
  on every route to the line (asserted earlier on every path, or a seed
  nothing retracts), so `::assert{inParty(isolde)}` then `@isolde: …` is
  clean there; a single-file `check` cannot see those and says so. The
  message names the condition and the guard to add
  (`@isolde{when="holds(inParty(isolde))"}`). `--deny W-CAST-ABSENT` works.
- **`emotion=` outside the speaker's `emotions:` is `E-BAD-ENUM`** (dsl
  0.24.0 §4): on a content line by that speaker, or a directive whose
  literal `character=` names them. A value the `emotion` enum itself rejects
  keeps its one existing error.
- **Components that write state: `effects: true`** (dsl 0.24.0 §4, T2-9).
  A component file declaring `effects: true` may `::set` / `::assert` /
  `::retract` (and use a state-writing plugin directive or another effects
  component). Each write is checked at every `::use` against the host's
  schema — undeclared path, `::set` type, `owner: engine`, relation
  vocabulary and domain, permissions — and reported at the `::use`. The
  writes compile into the host's commands where the `::use` sits, and a
  param-scoped `<match>` keeps only the arm the argument selects. Fact
  analyses in `check-project` see them there too. Guards and match subjects
  in the body still may not read state (`E-COMPONENT-STATE`), and a
  presentational component may not `::use` an effects component
  (`E-COMPONENT-BODY`). Without `effects: true` a write is still
  `E-COMPONENT-BODY`.
- **`speaker` component params** (dsl 0.24.0 §4): `params: { who: speaker }`
  takes a cast id. `{{@who}}` renders the member's `name` (the id when it has
  none), while attributes and `<match on="@who">` see the id; the match
  ranges over the declared cast plus `narrator`, so `<when is="isolde">`
  arms can each `::set{run.approval.isolde += @delta}`. An id outside the
  cast is `E-CAST-UNKNOWN` with a did-you-mean. Without a declared cast any
  identifier is accepted. A def argument is `E-COMPONENT-ARG`: the name is
  chosen when the component expands.
- **`E-RELATION-RESERVED-NAME`** (dsl 0.24 T3-8): a relation named like a CEL
  call, macro or keyword (`has`, `holds`, `count`, `isSet`, `now`, …) is an
  error at its declaration; a query over one (`holds(has(lamp))`) now says
  why it cannot parse.
- **`W-RELATION-UNREAD` and `W-DEF-UNUSED`** (dsl 0.24.0 §7, T3-16):
  `check-project` advisories, once per project at the declaration (the
  schema file line, or the document's own frontmatter key). A declared,
  non-reserved relation that is asserted, seeded or derived but that no
  condition queries (`holds` / `count` / `countDistinct`), no rule body uses,
  and no def reads records facts that change nothing; a declared `@def` that
  no content, other def, or rule guard references is dead. Play scripts and
  tests are not reads.
- **`lute scenario knowledge` covers every guard slot** (dsl 0.24.0 T3-1).
  Line `when=`, `<choice when>`, `<when>` arm tests, `::next`/`::set`
  `when`, `<on when>`, reward `when`, quest `start`/`fail` and objective
  `until` join beat, entry and objective guards, grouped per document with
  their source line. `--for` takes a scene (every guard in it),
  `quest:<id>`, and `<scene>#<branch>.<choice>` for one choice (was: exit 2
  on a scene without a fact-guarded frontmatter `when`). Constants the
  positive premises force are propagated into a negated premise
  (`not liar(hollis)`, not `not liar(_)`); a negation reads "holds unless
  defeated" with one "defeated when X is asserted by …" line per defeater,
  or "always holds (nothing produces X …) — cannot be defeated" (was
  `NO PRODUCER`, which read as a warning). A derived atom's rules print once
  per report; later mentions say "traced above under …". JSON elements gain
  `for` (their handles) and `line`; a `!holds(X)` read is listed as `!X`.
- **Provenance in `--explain`, `lute lore` and the envelope** (T3-2). An
  asserted leaf names who asserted it and when (`asserted by entry
  `keeperLog2`, step 4`, `engine step 7`; JSON `assertedBy`), and an absent
  negated premise some rule could conclude is expanded one level (each rule
  with the premise that keeps it from firing; JSON `attempts`). `lute lore`
  shows each entry's and beat's `when` and, when a relation is derived, a
  Derived section: every conclusion the rules can reach, its rule instances,
  the evidence it rests on and the conditions it gates (JSON `derived`).
  `lute scenario envelope`'s Facts rows use the knowledge wording
  (`seed facts …; reserved — the engine asserts it; derived by 1 rule`;
  was `asserted by: facts: seed`, silent on `reserved`).

- **Bridge answers in play, test and trace** (dsl 0.24.0 §5, T2-10). A play
  script takes `bridges: { <tag>: [ {<field>: value}, … ] }` at the top level
  (consumed in call order across the play) and per step (consumed first by
  that step's calls; answers the step leaves unconsumed fail it). Each answer
  gives exactly the `bridgeResult` fields the call's effects read, typed by
  the result slot — an unknown tag, a stray or missing field, or a misfit
  value is a usage error (exit 2) at script load. Answers land in `scene.*`
  result slots (a `state:` seed of `scene.*` stays refused), and the
  transcript shows `(bridge answered: passed=true, margin=3)`. `*.test.yaml`
  and trace mocks take the same key (`E-TRACE-MOCK-UNDECLARED` /
  `E-TRACE-MOCK-TYPE`, also in `check-project`'s `mocks/*.yaml` pass), and
  `lute run --mock` answers calls from it (docs/runtime/bridge-protocol.md).

- **Enum member display labels** (dsl 0.24.0 §1, T2-1). A long-form enum
  (a schema's `enums:` or a plugin's `enums/*.yaml`) takes
  `labels: { <member>: <text> }`, and `{{path}}` of a state path typed
  against it (`run.wd: { type: { domain: weekday } }`) renders the label —
  `Today is Sunday.`, not `Today is sun.`; a member without a label renders
  its id. `lute play`/`lute run`, `lute trace` and `lute test` agree. The IR
  state entry gains `labels` (omitted when none is declared, so existing
  artifacts and `capabilityVersion` stamps are unchanged). A label for a
  non-member is the new `E-ENUM-LABEL-NOT-MEMBER`; a non-string label in a
  schema document is `E-META-VALUE`. A state path typed `{ domain: X }` now
  counts as reading `X`, so it no longer draws `W-DOMAIN-UNREAD`.

- **Integer `%` joins the CEL profile** (dsl 0.24.0 §1, T2-1). `run.day % 7
  == 0` and a def `wd: "run.day % 7"` (typed `number` by inference) are
  clean; both operands must be integers, so a non-number operand (`'a' % 2`,
  `run.flag % 2`) or a fractional literal (`run.day % 2.5`) is the new
  `E-CEL-TYPE`. `%` was `E-CEL-PROFILE`. The IR `expr` gains the binary op
  `%`. Trace, test, play and the condition decider evaluate it as the
  truncated integer remainder (`-7 % 3 == -1`); a fractional value or a zero
  divisor is unknown (docs/runtime/cel-and-facts.md).
- **`lute calendar` gains `visited()` axes, per-occasion axes, a presence
  grid and a never-presented list** (T3-13). `--axis
  "visited('rock.arrival')=true,false"` puts a scene or bundle-beat id in or
  out of the cell's visited set, so an `after: visited(…)` beat can be read
  both ways. `--occasion dayEnd@run.day` varies only `run.day` for `dayEnd`
  (evaluated where the other axes are at their first value, or at
  `@run.day,run.slot=night`; blank / absent elsewhere) — a once-a-day
  occasion no longer repeats in every slot row. `--facts at` (repeatable)
  prints who is where in every cell (a row per first argument, a column per
  cell; a per-cell `facts` map in `--json`, a `facts:at` column in `--csv`).
  The report adds the beats eligible in some cell but presented in none,
  with what was presented over them (`neverPresented` / `beatenBy` in
  `--json`, a second table in `--csv`).
- **A declared clock** (dsl 0.24.0 §1, T2-1). A schema MAY declare
  `clock: { day, slot, slots, raise?, week? { length, first, labels } }` over
  two `owner: engine` state paths; a malformed clock, an undeclared /
  mistyped / content-owned `day` or `slot`, `slots` that are not the slot
  enum's members, an unknown `raise` occasion, or a second clock is the new
  `E-CLOCK-DECL`. Content reads the reserved, read-only `clock.index`
  (`(day-1)*len(slots)+slotIndex`), `clock.weekday` and `clock.weekdayLabel`
  (with a `week:`) in any condition or interpolation; `::set` of a `clock.*`
  path is `E-QUEST-RESERVED-WRITE`. Scene and bundle beats take `once: day` /
  `once: slot`, entries `once="day"|"slot"` — spent until the clock's day /
  slot changes (`lute play` prints `once: day — already presented today`);
  without a clock they are `E-BEAT-ATTR`. The artifact and project index
  carry the declaration as `clock` (omitted without one); `lute run` / `lute
  play` / `lute trace` derive the `clock.*` values from the live day / slot.
- **`lute play` advances the clock: `advance:` and `include:` steps** (dsl
  0.24.0 §1, T2-1). `advance: slot`, `advance: <n>` or `advance: day` moves
  a declared clock forward in one step — writes its day / slot paths
  (wrapping slots into the next day), settles the quests (a `by` the new
  time passes fails there), then raises the clock's `raise` occasion as an
  `occasion:` step would, taking its `pick` / `choose` and selection
  `expect:` — the step's `presented:` lists every beat the step presented,
  each `dayEnd` / `dayStart` raise's then the final raise's, in order
  (summer R2, lighthouse N15), while `winner:`, `offered:` and `notOffered:`
  judge the final raise, where the clock stops. The transcript prints `advance slot: day 1 (Mon) night → day 2
  (Tue) morning`; `--json` carries `advance: { by, from, to, writes, quests
  }` beside the raised occasion's fields. An `advance:` step may carry the
  `engine:` writes of the same moment (`advance: day` + `engine: { state: {
  run.leg: 3 } }`): they land where the clock arrives — after every
  `dayEnd` / `dayStart` the advance raises on the way (that evening's
  `dayEnd` still reads the day it closes, ember R3), before the final
  settle and raise — and one settle follows both (a write to the clock's
  own day / slot path there is a usage error). An `occasion:` step raising
  the clock's own `dayEnd` / `dayStart` prints a note: the next `advance:`
  across that midnight raises it again (summer R1; `--json`: the step's
  `notes`).
  An `engine:` step that moves `clock.index` backward is a usage error (exit
  2). `- include: <file>`
  splices another file's steps in place (relative to the including file;
  a cycle is a usage error). `lute calendar --axis clock[=d1..d2]` expands
  to every slot of those days, in clock order (bare `clock`: one week).
- **`::set{… when="…"}` — a guarded write** (dsl 0.24.0 §1, T2-1).
  `::set{run.aff.wren += 1 when="run.warmed.wren < run.day"}` writes only
  when the condition holds, replacing the `<match>`/`<when>` ritual around a
  single write. The guard is checked like a line `when=` (bool, `$` out of
  scope, `E-ARM-DEAD` when provably false) and is never a definite
  assignment, so a later read of an undefaulted path stays `E-MAYBE-UNSET`.
  No IR change: it compiles to the same one-arm `match` a gated line does;
  `lute play` prints a skipped write as `skip set <path> … — when: false`.
  Inside a `<track>` a guarded `::set` is `E-TIMELINE-CONTENT`. Any other
  attribute in a `::set` body is still expression text, and its
  `E-CEL-PARSE` now names `when=` as the one attribute there is.
- **Schemas and editor support cover 0.24.** `schemas/lute-ir-0.24.schema.json`
  documents every additive 0.24 IR field: the artifact's `clock`
  (`$defs/clockDecl`, the same shape as `project.index.json`'s `clock`),
  `once: "day"|"slot"` on scene beats, entries, bundle beats and index beat
  rows, `ObjectiveEntry.until`, `QuestCmd.activate`/`complete`,
  `OnCmd.target`, `AcceptCmd.applies`, placeholder `format`,
  `StateEntry.labels` and CEL `%`; the artifacts `lute compile` emits for them
  validate against it. `schemas/lute.schema.json` accepts a schema's `clock:`
  and a cast entry's `present`/`emotions`. The LSP offers and documents
  `<objective until>`, `<quest activate complete>`, `<on target>`,
  `::accept{at}`, and `once` `day`/`slot` on entries and bundle beats.
- **Subquests taken up in dialogue: `<quest activate="accept">`** (dsl
  0.24.0 §2, round 3 F6/F7). A child quest declaring `activate="accept"` does
  not activate with its parent: it waits for an `::accept` (or an `accepts:`
  mock) and activates only while its parent is `active` — an accept while
  the parent is not active is spent without effect, and says so: `lute
  trace` / `lute test` note `accept of `c` spent: its parent quest `p` is
  never active …` for an `accepts:` mock, and `lute play` prints `note:
  accept of quest c spent — its parent quest p is not active yet …` (`--json`:
  an `acceptSpent` record) (ember N15). `activate="accept"`
  beside `start` is `E-ATTR-TYPE`, and `::accept` of a child that activates
  with its parent is now `E-ACCEPT-TARGET` naming the parent (it was a silent
  no-op). `lute trace --accept` takes such a child. IR: `QuestCmd.activate`
  (omitted by default).
- **Alternatives: `<quest complete="any">`** (dsl 0.24.0 §2, F6). The quest
  completes when any one required objective — a child quest or a plain
  objective — is done; its other still-`active` children then fail with
  `failedBy: superseded` (a child nobody accepted stays `unset`), so the
  author no longer copies the cascade into every sibling's `fail=`. Its
  synthesized `fail` is the conjunction over every required objective (a
  child's `quest.<c>.state == 'failed'`, any other's
  `quest.<q>.objectives.<o>.failed`): one failed alternative leaves the
  quest open. Default `complete="all"`. IR: `QuestCmd.complete` (omitted by
  default).
- **Why a quest failed: `quest.<id>.failedBy` and
  `quest.<id>.objectives.<o>.failed`** (dsl 0.24.0 §2, F20). Reserved
  read-only paths: `failedBy` reads `unset` until the quest fails, then
  `fail`, `by`, `until`, `cascade` or `superseded`; `failed` reads `true`
  once the objective's `by`/`until` failed it. Any condition or
  interpolation may read them (an epilogue that tells a superseded
  alternative from a missed deadline); writing is `E-QUEST-RESERVED-WRITE`,
  declaring `E-QUEST-RESERVED-DECL`. `lute run`/`play`/`trace`/`test` agree,
  a run-tier quest's `newRun` reset clears both, and `lute run --json`'s
  failing `quest` record carries `failedBy`. The paths are typed by shape and
  never enter the IR `state` table.
- **`<on event="E" target="…">`** (dsl 0.24.0 §2): the handler runs only when
  the same-named occasion is raised for that target — never for a plain
  `event:` step, another target or a lifecycle transition (`lute play` /
  `lute run`, and `lute trace` / `lute test` `occasions: [E@target]`). The
  target is checked like an `<objective on target>`'s (`E-BEAT-ATTR`: a
  quoted dotted id, a targeted occasion named `E`, inside its domain; never
  on a lifecycle event). IR: `OnCmd.target`.
- **Occasion `judge: before`** (dsl 0.24.0 §2, F20). An occasion declared
  `judge: before` judges its `on=` objectives and settles the quests before
  its beats are decided and presented, so an epilogue on it reads how its
  quests ended. Only the judging moves: the `<on>` handler bodies the raise
  answers — the same-named `<on event>` handlers and the `questComplete` /
  `questFailed` handlers of the quests it settles — run after the beats, so
  their narration follows the scene (each handler's `when` is decided where
  it fired). `lute play` prints those quest transitions right under the step
  header, before the candidates, and `--json` gives them as the step's
  `judgedBefore` (the step's `quests` are the ones after, with the handler
  bodies). The default `after` keeps the 0.21 order and every existing
  `capabilityVersion` stamp.
- **`::accept{quest="…" at="nextRun"}`** (dsl 0.24.0 §2): the acceptance is
  queued and applies right after the next `newRun` reset, so a run-tier
  quest taken at a hub between runs survives the reset and activates in the
  new run's first settle. `lute play` prints `quest X accepted (queued:
  applies after the next newRun)` where it ran and `quest X accepted (queued
  at="nextRun")` under the new run (`--json`: the `newRun` step's
  `accepted`); `lute trace` marks the accept step `nextRun: true`. Any other
  `at` is `E-ACCEPT-TARGET`. IR: `AcceptCmd.applies`.
- **`W-QUEST-NEVER-ACCEPTED`** (new warning, dsl 0.24.0 §2, F8):
  `check-project` flags an accept-driven quest — no `start`, a root or an
  `activate="accept"` child — that no `::accept` in the project names and no
  `accepts:` mock (`mocks/*.yaml`, `*.test.yaml`) reaches. `--deny` works.
- **Bundle beats as `after:` predecessors, and accept anchors** (dsl 0.24.0
  §2, T2-13). `after: visited('<doc>.<beat>')` on a scene or quest resolves a
  bundle beat (it was `E-CONN-UNKNOWN-NODE` "is a bundle beat, not a scene").
  An accept-driven quest without `after=` is anchored at every scene, bundle
  beat and quest body that `::accept`s it: `lute scenario` draws an `accept`
  edge, and the quest leaves the unanchored list. An unresolvable quest
  `after=` now suggests dropping it rather than using `when`.
- **`W-DEADLINE-BEFORE-DONE`** (new warning, dsl 0.24.0 §2.1, LH N2): an
  `on=` objective with `by=` and no `until=` whose `done` provably implies
  `by` (`done="run.v == 'fell'" by="run.v != 'undecided'"`). `by` is judged
  at every settle and `done` only at the occasion's raise, so `by` fails the
  objective at the settle it comes true, before `done` is ever judged
  (unless both happen in the step that raises the occasion) — content
  written for 0.23.1's raise-only `by` can never complete. Anchored at `by=`,
  the message suggests `until="…"` with the same condition. Only a proven
  implication warns; `check` and `check-project` both report it, and
  `--deny` works.
- **Per-member defaults on a `per:` family** (dsl 0.24.0 §3, ER N1):
  `run.rep: { type: number, default: { _: 0, company: 1, dusk: -1 }, per: faction }`
  gives each member its own default; `_` is the fallback for members the map
  does not name. A key that is no member, a value that is not a scalar of the
  path's type, a member with neither its own value nor `_`, a map `default:`
  on a path without `per:`, and a list `default:` are `E-STATE-DECL` (a map
  default was silently accepted and yielded no default at all). `check`,
  `trace`, `run`, and `play` read the per-member values.
- **Component params in fact atoms** (dsl 0.24.0 §4, CR N5): an
  `effects: true` component may write `::assert{gifted(@who, @item)}` /
  `::retract{…}`. Each `::use` binds the params to its arguments and the host
  checks the bound atom there (membership, arity, `E-ARM-DEAD` routes); an
  argument that is not a constant (an entity/enum member id, `true`, `false`)
  is `E-COMPONENT-ARG` naming the atom. A `@param` in a fact outside a
  component body is `E-FACT-DOMAIN` (it was `E-DATALOG-PARSE`).

- **Day-granular clocks and day-boundary raises** (dsl 0.24.0 §1, round-3
  LH N7, SU N7). `clock:` may omit `slot:` / `slots:` (declared together):
  the clock counts whole days, a position reads `day 3`, `clock.index` is
  `day - 1`. `raise:` may be a map `{ slot, dayStart, dayEnd }` (a scalar is
  still the `slot` occasion): an advance raises `dayEnd` at every midnight
  it crosses, at the day's last slot with the day not yet advanced
  (`advance: <n>` walks there — it never skips a day's close; `advance: day`
  closes the day where the clock stands), then `dayStart` at the next day's
  first slot, then `slot` once where it stops. The transcript shows each as
  `── step N · day 1 (Mon) night · dayEnd`; `--json` lists them under
  `advance.days`.
- `lute calendar --occasion O@clock.day[,clock.slot=<slot>]` (or the
  clock's own paths, `O@run.day,run.slot=night`) with `--axis clock`
  evaluates `O` once per day at one slot (round-3 SU N3).
- `lute doctor` compares the `lute-lsp` beside the running `lute` with the
  one on `PATH` (`lute-lsp beside lute`): another build first on `PATH` —
  same version or not — fails with the fix (round-3 SU N10).
- **Cast `assume: true`** (dsl 0.24.0 §4, round-3 ER N5): beside `present:`,
  presence reads every negated `holds` of an engine-`reserved` relation in
  `present` (directly, or through a rule such as `inParty(P) :- …, not
  fell(P)`) as true. Without it a `present` over a reserved relation is
  satisfied only by a guard at each line or beat, since the checker does not
  know when the engine writes the fact; with it, a line after the engine
  event needs its own guard.

### Changed

- **An entry's `target=` on an untargeted occasion is metadata** (dsl 0.24.0
  §6, T2-12). `<entry on="keepsakes" target="item.compass">` on an occasion
  declared without `target:` is no longer `E-BEAT-ATTR`: the target says what
  the entry is about, and the entry answers every raise of the occasion
  (`lute play`, `lute calendar`, `lute beats` and the beat advisories treat it
  as untargeted; docs/runtime/beats-and-occasions.md states the engine rule).
  A scene's or bundle beat's `target` there is still `E-BEAT-ATTR`.
- **`by=` is judged at every settle, `on=` or not** (dsl 0.24.0 §2.1, T2-4).
  0.23.1 judged an `on=` objective's deadline only when its occasion was
  raised, so a player who never went back escaped it; now the settle after
  the deadline comes true fails the objective wherever the player is (`done`
  still wins a tie in the same settle — and in the step that raises an
  `on=` objective's occasion, whose presentations and settles judge that
  objective's `by` only after the raise judged its `done`, so a hearing that
  files the right verdict completes it). The old place-bound rule is the new
  `until="…"`: judged only when the objective's occasion (and `target`) is
  raised, after `done`. `until` without `on` is `E-BEAT-ATTR`. The IR's
  `ObjectiveEntry` gains `until` (omitted when absent). `lute run` / `lute
  play` print `<quest>.<objective> failed (until)` for an `until` failure
  (was always `failed (by)`), and the objective record carries `"failedBy":
  "by" | "until"`.

- **`lute trace` and `lute beats` print a def reference as the author wrote
  it** (T3-12). A `<match on="@weekday">` header reads `<match @weekday>`
  (was `<match (run.day == 1 ? 'mon' : …)>`), an arm or choice guard
  `(@atLeast(3))`, the coverage summary `arms 1/2 (@weekday @12:1)`, and a
  beat's `when` column `@runsAtLeast(2) && @firstDay`. The new `--expand` flag
  on both prints the expansion instead. `--json` keeps the expansion in
  `id`/`guard`/`label`/`when` and adds the author's text as `authoredId`/
  `authoredGuard`/`authoredLabel` (trace, only when an expansion changed it)
  and `whenAuthored` (beats).
- **An atomic `@def` argument is substituted without parentheses** (T3-12):
  `@atLeast(2)` over `user.runs >= n` expands to `(user.runs >= 2)`, not
  `(user.runs >= (2))`. A path, number, plain string literal or already
  parenthesized group splices bare; anything else (`a + 1`, `-2`, a call)
  keeps its parentheses. The compiled `expr` is unchanged; the IR `raw` text
  of a def called with such an argument is shorter.

- **`lute init --template investigation` is rebuilt on the `beats` skeleton**
  (T3-14). The old template still wrote dsl-0.3 `character:`/`season:`/
  `episode:` frontmatter with no `defaults:`, plugin, plays, tests or negation.
  It is now a small whodunit: a `case.occasions` plugin raising `arrive`, the
  targeted `examine` (`item.<evidence>`) and `interview` (`npc.<suspect>`) and
  `accuse`; evidence lore that `::assert`s what is on record; derived rules
  with stratified negation (a suspect is `cleared` by an alibi unless evidence
  contradicts it, the `culprit` is implicated and not cleared); an
  accept-driven quest; an accusation whose choices are guarded by
  `holds(culprit(…))`; `plays/the-case.play.yaml` with `expect:` and a scenario
  test. `check-project`, `test` and `play` pass as scaffolded, and its README
  shares the `beats` commands.
- **`lute new --dir` inside a project but not at its root is refused** (exit
  2, nothing written) with the spelling to use instead: `` `--dir` names the
  project; did you mean `lute new scene talk/tavi-shell`? `` (plus `--dir
  <root>` when the root is not the current directory). It used to write to
  `<root>/scenes/<name>.lute` without a word.
- **A dotted `lute new` name keeps its dots as the id**: `lute new scene
  isolde.night` writes `scenes/isolde.night.lute` with `id: isolde.night`
  (was `isoldeNight`); quest and lore document ids follow (`quest.a.b`). `-`
  still camel-cases within a segment.
- **`lute new quest` scaffolds an accept-driven stub** (no `start`, with a
  comment naming the `::accept{quest="…"}` that begins it); the new `--start`
  flag restores the auto-starting `start="true"` form.
- **A plugin directive's `lower:` is optional** (T3-7): absent means the
  generic `kind: "plugin"` passthrough, as documented — it used to be
  `E-PLUGIN-PARSE missing field lower`. A `{ kind: builtin, name: X }` must
  name a hook the core registers (`autoStage`, `cameraTransform`,
  `clearStage`, `end`, `mark`, `next`); any other name is `E-PLUGIN-PARSE`
  with a did-you-mean and
  "omit `lower:` for the generic passthrough" (it used to lower as a silent
  passthrough). The shipped examples and the plugin bridge guide that named
  unregistered hooks (`bridgeMinigame`, `mgart`, `bridgeServe`,
  `bridgeHostPanel`) drop `lower:` — a bridge directive binds its call with
  `bridge: { service, operation }` alone; their compiled records are
  unchanged (their capability stamp moves).
- **Plugin export files reject unknown keys** (T1-3). Every declaration a
  plugin exports (directives and their attrs/state/effects/bridge, state
  shapes and templates, providers, bridge capabilities, defs, enums, events,
  frontmatter, asset kinds, stamp attrs, reward kinds, occasions, lints) is
  `E-PLUGIN-PARSE` on a key it does not know, with a did-you-mean:
  `report: { selct: all }` says ``unknown field `selct` … did you mean
  `select`?`` instead of loading as `select: first`. When the mapping holds a
  key with no value — an unquoted flow-map description split at its comma
  (`{ description: Pick one, the player picks one }`) — the error says to
  quote it and spells the quoted form. A `state/` file may now hold both
  `stateShapes:` and `stateTemplates:` (the second used to be dropped).
- **`capabilityVersion` moves for every document**: `lute.core` gains
  `::clear`, so the core capability stamp changes. The conformance fixtures
  are re-recorded (only the stamp moves in `artifact.json`; `quest-subquest`'s
  transcript gains `failedBy` on each failing `quest` record and the
  `quest.<id>.failedBy` state paths, and the fixture table lists it).
- **`lute play` and `lute run` name why a quest failed**: `quest X -> failed (by)`,
  `(until)`, `(fail)`, `(cascade)` or `(superseded)` (dsl 0.24.0 §2; was
  `quest X -> failed`). `lute trace`'s decision for a quest failed from above
  reads `superseded from quest.P` for a `complete="any"` parent's untaken
  alternative (still `cascade from quest.P` otherwise).
- **A bridge result is answered by `bridges:`, not a `scene.*` state seed**
  (dsl 0.24.0 §5, ember N8). Migration: a 0.23.1 `*.test.yaml` or trace mock
  that seeded a plugin call's result slot — `state: { scene.check.guards.passed:
  true }` — no longer decides the guard; the call's answer is read from
  `bridges:` only, so the walk stops unresolved with a `bridges:` hint.
  Replace the seed with `bridges: { check: [ { passed: true, margin: 3 } ] }`
  (one answer per call, in call order, every field the result shape
  requires). `lute play` refuses a `scene.*` seed outright (exit 2).

- `advance: day` is documented to land on the next day's first slot from
  any slot, and a clock's `slot` raise fires once per advance, never at the
  slots it passes (round-3 LH N7).

### Fixed

- **A component can index a `per:` family by its param** (ER N6).
  `::set{run.approval[@who] += 1}` (and a `run.approval[@who]` read in the
  component's CEL) binds to `run.approval.isolde` at each `::use`; an
  argument that is not a member of the family's kind is `E-COMPONENT-ARG` at
  the `::use`, naming the kind's members. It was `E-UNDECLARED` plus a
  misleading `E-CEL-PARSE`.
- **A plugin directive lowered by a builtin hook runs that builtin.**
  `lower: { kind: builtin, name: clearStage }` compiled to a `kind: plugin`
  passthrough and cleared nothing; `compile` and `trace` now treat the
  directive as the core directive the hook belongs to (`clearStage` →
  `::clear`, `autoStage` → `::auto`, `cameraTransform` → `::camera`, `end`,
  `mark`, `next`).
- **`::clear` rejects timing attributes.** `::clear{duration="0.5"
  wait="true"}` checked clean and the keys were dropped; `duration`, `delay`
  and `wait` are now `E-UNKNOWN-ATTR` on `::clear`, which takes no attributes.
- **A cast `present:` on an undeclared path is `E-UNDECLARED`** (plugin or
  schema cast), reported at the member's line in each document that does not
  declare the path, instead of a `W-CAST-ABSENT` suggesting that same
  undeclared guard.
- **Unresolved components get a did-you-mean** (CR F5). A `components:`
  import that does not resolve names the project's component file with that
  name (or the nearest one), spelled from the importing document
  (`did you mean ../../components/gauge.component.lute?`), and says a
  document's own `components:` resolves against its directory while
  `defaults: components:` resolves against lute.project.yaml's. A `::use` of
  an undeclared component names the nearest declared one.
- **`lute new` from a subdirectory no longer blames `--dir`** (CR N7). With
  no `--dir`, the refusal says the project was taken from the current
  directory, names it, and says to run from the project root or pass
  `--dir <root>`. With `--dir`, it names the directory it resolved to.
- **Exhaustiveness honors a beat's `when:`** (LH N1). `E-NONEXHAUSTIVE` now
  reads the same narrowed domain `E-ARM-DEAD` and `W-OTHERWISE-DEAD` do: under
  `when: "run.verdict != 'undecided'"` a `<match on="run.verdict">` without an
  `undecided` arm is exhaustive, so deleting the dead arm is clean. A body
  that writes the subject keeps the whole domain. `W-OTHERWISE-DEAD` names
  "the domain left by the body's `when` guard" when the guard did the
  covering.
- **`clock.weekday` and `clock.weekdayLabel` are typed** (SU N1, dsl 0.24.0
  §1). `clock.weekday` is the whole numbers `0..length-1`: `is="0"` …
  `is="6"` (or ranges) is exhaustive with no `<otherwise>`, a gap is named
  (`` `6` is not covered ``), `is="7"` is `E-WHEN-LITERAL-DOMAIN`, and
  `clock.weekday == 7` is a dead guard. `clock.weekdayLabel` is the enum of
  `week.labels`, so label arms are checked like any enum (`is="Sundy"`,
  `== 'Sundy'`).
- **`W-DOMAIN-UNREAD` sees kind reads and lands on the schema** (CR N1, ER
  N10). A kind used as a `per:` index, a sub-kind's `subsetOf:` parent, and a
  kind atom in a rule body or a `holds(…)` condition now count as reads. The
  warning is reported at the declaration's line in the schema that declares
  it (or the document's own `entities:` / `enums:` key), not at `1:1` of the
  first importer.
- **An entity kind in a rule body now derives at runtime.** `inParty(P) :-
  companion(P), …` with `companion` an entity kind (dsl 0.3.0 §3.1's unary
  domain predicate) passed `check` but derived nothing in `lute run`/`play`/
  `trace`/`test` — the evaluator looked for `companion(…)` facts, and none are
  ever asserted under a kind's name. A kind atom is now a membership test over
  the kind's members (IR `entities`), in the join, under `not`, and in
  `--explain` (`companion(isolde) — entity kind `companion``);
  docs/runtime/cel-and-facts.md states the rule for engines.
- **A `@def` in a rule `cel()` guard works** (T1-1). `litA(lamp) :-
  cel("@firstDay")` passed `check` and then never derived: the guard was
  evaluated unexpanded, read undecided, and the rule was silently dropped.
  Rule guards now expand defs (with arguments) against the project's def
  table, so check, trace, test and play all read the expanded body; the
  expanded body still passes the rule-guard firewall. An undefined def, a
  wrong argument count or `$` in a rule guard is the new
  `E-RULE-GUARD-DEF`. A rule guard that is still undecided at play time no
  longer reads as "no such fact": a guard querying that relation is unknown
  and play halts there, naming what would decide it.
- **A maybe-unset read through a `@def` is reported** (T1-4). A def is
  checked where it is used: `{{@lastF}}`, `when="@lastF > 30"`, a
  `::use` argument `fathoms=@lastF`, a `::set` right-hand side and a
  `<match on="@def">` subject all read what the def body reads, so
  `lastF: "prev.run.depth * 10"` used unguarded is `E-MAYBE-UNSET` at the use,
  naming the path and the def. A guard inside a def body counts too.
- **Guards narrow consistently** (T1-7). A scene beat's `when:`, a bundle
  `<beat when>` and an entry `when=` hold throughout their body: its
  `isSet(…)` guards prove the body's reads and `<match>` subjects (no
  `E-MAYBE-UNSET` / `E-UNSET-UNCOVERED`), and a `<match>` arm whose every
  value the guard rules out is `E-ARM-DEAD` (`when: "run.outcome ==
  'diving'"` makes `<when is="died">` dead unless the body writes
  `run.outcome`). Inside one expression a presence guard proves the reads its
  short-circuit protects: `run.a == 1 || (isSet(run.o) && run.o == 'won')`,
  `isSet(run.o) ? run.o == 'won' : false` and `!isSet(run.o) || run.o ==
  'won'` are clean — the proof never reaches the guarded body. And
  `prev.run.*` is one snapshot: once any `prev.run.<p>` is known present,
  every `prev.run.<q>` whose `run.<q>` has a `default` is too.
- **`<match on="@def">` has a domain** (T1-5). A def whose body is one state
  path (`wd2: "run.wd"`) matches like that path; any other def takes its
  declared or inferred type (`type: { enum: [a, b, c] }` → its members). An
  exhaustive def match is no longer `E-NONEXHAUSTIVE`, and a typo arm is
  `E-WHEN-LITERAL-DOMAIN`. `E-NONEXHAUSTIVE` now names the uncovered members
  (`` `b`, `c` are not covered ``).
- **Schema problems are reported once, at the schema's line** (T3-6). A
  diagnostic about an imported declaration (`W-DERIVE-NO-RULES`,
  `E-ENTITY-KIND-CLASH`, a rule's `E-DATALOG-UNSAFE`, …) used to repeat at
  `1:1` — or at an unrelated line — of every importing document. It now
  names the schema file, carries the declaration's own line, and
  `check-project` folds the importers' copies into one (`+N more callers`).
  A relation whose only rule failed to parse no longer also draws
  `W-DERIVE-NO-RULES` or emptiness verdicts (`E-OBJECTIVE-UNSATISFIABLE`, a
  dead guard) — the parse error is the one report.
- **`W-BEAT-SHADOWED` catches an untargeted beat beaten on every target**
  (T1-8): on an occasion whose target domain is closed, an untargeted beat
  that, at every `<prefix>.<member>`, loses to an earlier always-eligible,
  never-spent beat for that member is reported, naming the shadower per
  target.
- **`W-BEAT-PRIORITY-TIE` no longer fires on beats `once` or a rule schedule
  keeps apart** (T3-3): a beat's `once` joins its eligibility (an entry's
  `once="user"` needs `!entry.<id>.everRead`, `once="run"`
  `!entry.<id>.read`; a scene's or bundle beat's `once: user`
  `!visited('<id>')`), so `tName once="user"` and `tDeath … &&
  entry.tName.everRead` are exclusive; and `holds(at(sol, radio))` of a
  derived atom whose rules are all ground `cel()`-only schedules stands for
  those guards, so beats on two schedule slots no longer tie.
- **`W-BEAT-ONCE-RUN-USER` sees user-tier relations and quests** (T3-4): a
  defaulted `once: run` beat gated only on `holds` / `count` of a `tier:
  user` relation, or on `quest.<id>.*` of a user-tier quest, now warns like
  one gated on `user.*`.
- **A scene beat's frontmatter `when:` has its quest and entry ids checked**
  (T1-6): `when: "quest.lampOot.state == 'active'"` is
  `W-QUEST-REF-UNKNOWN` (and `entry.tomasOyl.everRead` `W-ENTRY-REF-UNKNOWN`)
  anchored at the id in the frontmatter; both codes add a did-you-mean.
- **A component `{{@param}}` bound to a def whose name is a different length
  renders whole in `lute trace` and `lute test`** (T1-12). `::use{component=
  "gauge" fathoms=@bondTimesTen}` traced as `The gauge shows 20Ten}}
  fathoms` (the rebound interpolation kept the param token's span), so a
  `transcriptContains` that `lute play` passed failed in `lute test`. Every
  interpolation of a bound component line now carries its span in the
  rewritten text.

- **An unanswered bridge no longer walks the default arm** (T1-14). `lute
  play` walked a `<match>` over a plugin call's result slot to its default
  arm (printing it) and only then halted; it now halts AT the call (exit 3),
  naming the tag and the `bridges:` answer to give. `lute trace`/`lute test`
  read the state-shape default (`passed: false`) and completed at exit 0; an
  unmocked result slot now reads unknown, so a guard over it halts the trace
  incomplete with a `bridges: { check: [ { passed: <bool>, margin: <number> } ] }` hint.

- **A plugin that failed to load is no longer reported as not installed**
  (T3-7). When a package's `plugin.yaml` parsed but an export did not, the
  follow-up `E-PLUGIN-MISSING-ACTIVE` says ``plugin `x` … failed to load (see
  E-PLUGIN-PARSE above)`` rather than telling the author to install it.
- **`::auto{character}` and `::camera{focus}` are checked against the cast**
  (dsl 0.24.0 §4, T1-10). With a cast declared, a staged character outside it
  is `E-CAST-UNKNOWN` with a did-you-mean, like a speaker — timeline clips and
  match/choice bodies included. `::auto{character="marra"}` used to pass while
  `@marra:` did not.
- **XML character references in attribute values are decoded** (T1-11).
  `label="&quot;Quoted&quot;"` used to ship the entity text literally; a
  quoted value now decodes `&quot;` `&apos;` `&amp;` `&lt;` `&gt;` and
  `&#NN;` / `&#xHH;` (one pass, so `&amp;quot;` is `&quot;`). Any other `&`
  — a bare `&`, `&&`, `&nbsp;` — stays literal. `\"` still works.
- **A guard that assumes an entry was read knows what the entry asserted**
  (dsl 0.24.0 §6, T2-14). Under `entry.X.read` (or `== true`) the fact
  analysis adds the facts X's body asserts on every route (and nothing
  retracts) to the must set, so a redundant `holds(…)` under it is
  `W-FACT-GUARANTEED` and its negation a dead guard. `entry.X.everRead` adds
  only the `tier: user` / `tier: app` ones, which a new run keeps.
- **`W-QUEST-HANDLER-DEAD` also names a handler that can only fire on a
  completed quest** (T3-11). An `<on event="E" when="G">` whose `G` contains
  every required objective's `done` (any one under `complete="any"`), after
  `@def` expansion, never runs: the quest settles complete as soon as `G`
  holds, and an event reaches only an active quest's handlers. `lute check`
  reports it too; the lifecycle events and quests with an `on=` or `quest=`
  objective are exempt.
- **An unsupported `lute calendar --axis` lists the axis kinds** (T3-13): a
  declared state path, `quest.<id>.state`, `quest.<id>.objectives.<oid>.done`,
  `holds(<fact>)`, `visited('<id>')` — with a did-you-mean for a mistyped
  state path. A derived atom reads once: `` `trusted(ada)` is derived by rules
  and cannot be asserted `` (was `` `trusted(ada)` `trusted` is derived… ``;
  play's `facts:` / `engine:` seeds say it the same way).
- **`transcriptContains` / `transcriptLacks` match presented lines only, in
  one form** (T1-2). `lute play` and `lute test` both match against the
  content lines that actually played, one `@speaker: text` per line — no
  step headers, candidates, staging, notes or `skip @x "…" — when: false`
  lines (a guarded line that never played used to satisfy
  `transcriptContains`). A scene test used to match the trace's `@x  text`
  rendering and a play `@x: text`; `"@narrator: Always shown."` now works in
  both.
- **`lute test`'s `offered:` sees a hub's options** (T1-13): every choice
  eligible at each visit, unioned, like a branch's (was always `[]`).
- **`eligible:` in `lute test`** (T3-5): an `eligible:` expectation silences
  the "not eligible under these mocks" note it answers (the note now names
  the map form, `eligible: { <id>: false }`); a map key naming an entry or
  bundle beat of the file the test did not present is judged alone under the
  same mocks, and a lore test may carry a map-form `eligible:` without
  presenting anything. New `expect.accepts: [quest ids]` asserts the quests
  a scene's `::accept` took (as a set).
- **Play harness details** (T3-10, T3-11): a step `expect.options: {
  <branch or hub>: [ids] }` asserts the options offered in that step (as a
  set); a `select: all` occasion with nothing eligible needs no `pick:` (it
  passes as `pick: none`; a non-empty list without one halts naming the
  offered beats); a `newRun` prints each `prev.run.*` value it snapshotted
  (`--json`: `prevRun`) and a note for an accept-driven run-tier quest it
  resets while active with no objective done or failed (`resetUnjudged`; a
  `start=` quest, which `at="nextRun"` cannot take, gets none); an atom YAML split at
  its comma (`facts: [heard(tavi, regent)]`) says to quote it; an `<on
  event>` handler of a quest that already settled prints `<on event=E> of
  quest Q skipped — quest complete` instead of vanishing.
- **`lute trace --project` settles quest existence** (T3-15): a foreign
  quest the project declares no longer gets "existence is unverified"; one
  it does not declare says so, with a did-you-mean.
- **`lute test --coverage` counts what an `advance:` step presented**
  (lighthouse N3). The occasion a clock's `raise:` fires after an `advance:`
  step presents beats exactly as an `occasion:` step does, but coverage read
  only `occasion:` steps, so a scene a play reached through `advance: day`
  was listed as an untested document. The header now says what a play feeds:
  `coverage over N traced path(s) and M play(s) (plays count toward
  documents presented only, not branches or arms):` — the branch/hub and arm
  rows come from traced paths alone.
- **The unanswered-bridge hint is an answer the loader accepts** (ember-road
  N7). `lute trace`/`lute test` hinted `bridges: { check: [ { passed: <value>
  } ] }`, and supplying exactly that was refused for lacking `margin`. The
  hint now lists every field the call's result shape requires, each with a
  type placeholder — `{ passed: <bool>, margin: <number> }` (an enum is
  `<one of: a|b>`) — and `lute play`'s halt spells its answer the same way,
  as does its load-time refusal of a `bridges:` answer missing a field.
  A `bridges:` error in a `*.test.yaml`, a `trace --mock` file or a
  `mocks/*.yaml` is anchored at the offending tag key, answer or field key in
  that file (`tests/t.test.yaml:6:21`), not at `<document>:0:0`; the JSON
  diagnostic carries `"provenance": "mock"`. An answer missing a field is now
  `E-TRACE-MOCK-TYPE` (was `E-TRACE-MOCK-UNDECLARED`), and its message shows
  the whole typed answer.
- **`lute test --coverage` names a `<match>` over a `@def` as authored**
  (ember N14): ``match `@wrenWithUs` (…)``, as `lute trace` prints it, not
  the def's expansion ``match `(holds(inParty(wren)))` ``.

- `lute scenario knowledge` reads an entity-kind atom in a rule body as a
  membership premise (`suitor(sol) — entity kind suitor; sol is a
  member`), not an "undeclared relation", and lists the rule's `cel()`
  premise with the member it reads (`cel("run.aff.sol >= 3") — state
  condition on run.aff.sol, decided at run time`); `--format json` marks
  kind premises `entityKind` and lists `cel` premises (round-3 CR N2, SU N6,
  ER N9).
- `lute beats` lists bundle beats with `once="day"` / `once="slot"`
  (round-3 SU N2).
- A `clock:` whose `day` / `slot` path is undeclared, mistyped or not
  `owner: engine`, or whose `raise` names an undeclared occasion, is
  `E-CLOCK-DECL` from `lute check <schema>.yaml` at the `clock:` line (it
  said `ok`; the occasions are the enclosing project's, when there is one).
  `check-project` reports it once, attributed to the schema's `clock:`
  line, instead of at `1:1` of every importing document.
- `lute check <schema>.yaml` reports everything an importer's check would
  about the schema — `E-ENUM-LABEL-NOT-MEMBER`, `E-ENTITY-KIND-SHAPE`,
  `E-ENTITY-KIND-CLASH`, `E-FACT-DOMAIN`, the clock — at the schema's own
  lines (it said `ok`). In `check-project` an imported enum's
  `E-ENUM-LABEL-NOT-MEMBER` is folded into one report at the schema's line
  like the others, instead of `1:1` of every importer.
- `E-RULE-GUARD-DEF` (and a malformed rule) in a document's own `rules:` is
  reported at the rule's line, not `1:1`, when the rule quotes its
  `cel("…")` guard.
- The `E-BEAT-ATTR` hint for an entry's `once="day"` / `once="slot"` without
  a clock suggests `run` / `user` or omitting `once`, not `false` (which an
  entry refuses).
- A trace / test mock that seeds a derived `clock.*` path is
  `E-TRACE-MOCK-UNDECLARED`, naming the clock's day / slot paths to seed; it
  was accepted and contradicted them.
- `lute calendar` says `undecided (…)` and `lost to an undecided cell (…)`,
  naming the beat whose `when` is unknown, where it printed `? over quiet` /
  `lost to ?`.
- **`W-CAST-ABSENT` is precise** (dsl 0.24.0 §4; round-3 SU N4/N5, ER
  N3/N4, CR N4, LH N4). Measured on the four reviewed games, with `present:`
  declared as the reviewers had it and their workaround guards removed:
  Drowned Crown 12 → 0, Summer Station 5 → 0, Skerry Rock 1 → 1 (Ada's
  handwritten log line, which needs `{vo}`), Ember Road 11 → 9 (companion
  lines before the battle whose `present:` reads the engine-reserved `fell`;
  `assume: true` clears them). Every real catch the reviewers fixed is still
  reported.
  - An `::assert`/`::retract` voids only a guard atom it can falsify on that
    path: same relation with unifiable arguments, or through a rule where the
    written relation is a premise, moving it the wrong way. Asserting
    `recruited(tomas)` keeps `holds(inParty(mara))`, and so does asserting a
    positive premise. Each `&&` conjunct of a guard stands alone.
  - A write in one `<choice>` or `<match>` arm no longer reaches its
    siblings. A guard survives the branch when it survives every arm.
  - A guard's `holds(A)` over a derived relation implies its rules' bodies
    (`holds(inParty(mara))` gives `!holds(departed(mara))`). A relation
    whose rules are ground once bound, such as a `cel()`-only schedule, reads
    as the disjunction of those bodies, as `W-BEAT-PRIORITY-TIE` does. A
    `::set` of a path such a rule reads voids the guard.
  - A quest `<on>` handler assumes the quest's state at that event and the
    `start` conjuncts that stay true (`entry.<id>.everRead`, `visited(…)`).
    An entry's body assumes its own `everRead`/`read`. Under
    `check-project`, a beat assumes that every always-eligible `once` beat
    ranked above it on the same ladder has been spent.
  - A `{vo}` line is exempt. An `{os}` line is still checked, because the
    speaker is in the scene, just out of frame. Two of Drowned Crown's real
    catches were `{os}` lines.
- **`W-CAST-ABSENT` sees a fact only its own beat produces** (round-3 ER
  re-verify R1). Under `check-project`, a beat, entry or bundle beat
  presented at most once per run starts with `!holds(F)` for every ground
  fact `F` that only it asserts. No other unit of the root, component
  documents included, can assert `F`, and no seed names it. The fact's
  relation is `tier: run`, or `tier: user` in a `once: user` beat; it is
  neither derived nor engine-`reserved`. The body's own `::assert{F}` ends
  the assumption on that path. So a `present:` such as
  `holds(inParty(wren)) || !holds(recruited(wren))` is satisfied before and
  beside the only choice that recruits her, even when another scene can
  make her depart or `fell` is reserved.

### Compatibility

- **Arms kept only to satisfy the checker are now `E-ARM-DEAD`.** A beat's
  `when:`, a bundle `<beat when>` and an entry `when=` now narrow their whole
  body, and exhaustiveness reads the same narrowed domain. The 0.23 workaround
  — a `<when is="undecided">` (or `<otherwise>`) arm the beat's `when` rules
  out, kept so `E-NONEXHAUSTIVE` would pass — is now `E-ARM-DEAD` (or
  `W-OTHERWISE-DEAD`). Delete the dead arm; the match stays exhaustive. A body
  that writes the subject keeps the whole domain.
- **`by=` is judged at every settle; the old rule is `until=`.** 0.23.1 judged
  an `on=` objective's `by` only when its occasion was raised. Now it fails the
  objective at the first settle where it holds, wherever the player is. Content
  written for the raise-only rule — a `by` that `done` implies, such as
  `done="run.v == 'fell'" by="run.v != 'undecided'"` — can never complete and
  is flagged `W-DEADLINE-BEFORE-DONE`; replace `by=` with `until=` (same
  condition), which is judged only at the occasion's raise, after `done`.
  `lute run` / `lute play` print `failed (until)` for it, and failing
  `quest` records carry `failedBy`.
- **Bridge results come from `bridges:`, not `scene.*` seeds.** A `*.test.yaml`
  or trace mock that seeded a plugin call's result slot (`state: {
  scene.check.guards.passed: true }`) no longer decides the guard: the walk
  stops unresolved with a `bridges:` hint, and `lute play` refuses the seed
  (exit 2). Replace it with `bridges: { check: [ { passed: true, margin: 3 }
  ] }` — one answer per call, every field the result shape requires.
- **Plugin `lower:` is optional, and the `builtin` workaround is rejected.** A
  directive without `lower:` is the generic `kind: "plugin"` passthrough (it
  was `E-PLUGIN-PARSE missing field lower`, and `schemas/lute.plugin.json`
  required it too). A `lower: { kind: builtin, name:
  X }` naming a hook the core does not register (`bridgeMinigame`, `mgart`, …)
  — the old way to satisfy that error — is now `E-PLUGIN-PARSE`; drop `lower:`
  (a bridge directive binds its call with `bridge:` alone; the compiled
  records do not change). A registered hook now runs its builtin:
  `clearStage` clears the stage as `::clear` does, where it used to compile to
  a passthrough. Plugin export files also reject unknown keys
  (`E-PLUGIN-PARSE` with a did-you-mean), so a misspelled key that loaded as
  its default now fails.
- **Cast presence is a new warning.** A cast entry that declares `present:`
  makes every line by that speaker whose guards do not imply it
  `W-CAST-ABSENT` (`--deny W-CAST-ABSENT` works). Projects whose cast has no
  `present:` are unaffected. Guard the line (`@isolde{when="…"}`), or declare
  `assume: true` when `present` reads an engine-reserved relation whose
  negation holds until the engine writes it.
- **`judge: before` moves when quests settle, not when handlers run.** On an
  occasion declared `judge: before`, its `on=` objectives are judged and the
  quests settled before its beats are chosen, so an epilogue reads the ending;
  the `<on>` handler bodies of that raise still run after the beats. The
  default `after` keeps the 0.21 order, and only a snapshot that declares
  `judge: before` changes `capabilityVersion` for it.
- **`capabilityVersion` moves for every document.** `lute.core` gains
  `::clear`, so every capability stamp changes; engines or build caches keyed
  on the stamp see new values. Compiled records are otherwise unchanged for
  documents that use none of the new syntax.
- **Schema file renamed; additive IR.** The version strings move to `0.24.0`
  and `schemas/lute-ir-0.23.schema.json` is renamed to
  [`schemas/lute-ir-0.24.schema.json`](schemas/lute-ir-0.24.schema.json)
  (`$id` updated). Every new field is optional and appears only when the
  source uses the feature: `clock`, `once: "day"|"slot"`,
  `ObjectiveEntry.until`, `QuestCmd.activate` / `complete`, `OnCmd.target`,
  `AcceptCmd.applies`, placeholder `format`, `StateEntry.labels`, and the CEL
  op `%`. Engines gate on MAJOR, so nothing widens; the tree-sitter grammar is
  unchanged.
- **Documents that were already wrong can redden.** A relation named like a
  CEL call (`has`, `holds`, `count`, …) is `E-RELATION-RESERVED-NAME`; an
  `::accept` of a child that activates with its parent is `E-ACCEPT-TARGET`
  (it was a silent no-op); a map `default:` without `per:` is `E-STATE-DECL`
  (it yielded no default); a maybe-unset read through a `@def` is
  `E-MAYBE-UNSET` at the use; a `@def` a rule guard cannot expand is
  `E-RULE-GUARD-DEF` (the rule used to be dropped silently).

## [0.23.1] - 2026-09-25

**Trace, test and play agree.**

A patch on the `0.23` line from a second dogfood round over three games. Where
`lute trace`, `lute test` and `lute play` answered one question three ways — a
deadline judged before its objective, a reward credit only play applied, an
occasion that never reached its quest's event handlers, a `::end` that stopped
the whole playthrough — they now give one answer, the one the runtime contract
specifies; the checker closes a few more gaps (`E-QUEST-TIER-MIX`, match-arm
narrowing, negation over stable derived facts). No syntax is added and the IR
shape does not move. See [`docs/versioning.md`](docs/versioning.md) for what
each axis earned.

### Changed

- `E-QUEST-TIER-MIX`: a subquest whose `tier` differs from its parent's is an
  error naming both quests and tiers (`check` when both share a document,
  `check-project` across documents) — a mixed tree locks for good.
- An `on=` objective's `by` deadline is judged only when its occasion is
  raised, after its `done`; in every settle `done` is judged before `by`.
  `lute trace`, `lute run` and `lute play` agree.
- `lute trace` / `lute test` apply a reward kind's `credits:` and walk objective
  completion bodies exactly as `lute play` does; play prints a whole credit as
  `= 1`, not `= 1.0`.
- `*.test.yaml`: `expect.facts` / `notFacts` (after derivation), `expect.eligible`
  for a presented entry/beat (and a note when one is presented though
  ineligible), `beat: <id>` presents a bundle beat; `E-TRACE-ENTRY` names the
  document's beats.
- `lute test` resolves scenario tests against the nearest `lute.project.yaml`
  (announced on stderr) when `--project` is absent, accepts one `*.test.yaml` /
  `*.play.yaml` file, and reports a test whose `file:` is missing as one
  `E-TEST-FILE` failure instead of aborting the suite.
- Play step `expect:` gains `quests`, `state`, `facts`, `notFacts`, judged right
  after that step settles, on any step kind; state misses quote both sides.
- `lute play`: a `::end` ends only the presentation (or quest handler) it runs
  in — the playthrough goes on with the next step. A new step `end: true` ends
  the playthrough (exit 0); later steps print as skipped and `--json` lists them
  under `skipped`. Scripts that relied on `::end` stopping the play add
  `- end: true` after that step.
- Raising an occasion also fires a same-named declared world event: every
  active quest's `<on event>` handlers run, then the occasion judges its `on=`
  objectives — in `lute play`, `lute run` and `lute trace` alike.
- `lute play` prints staging as authored (`::bg{…}`, `::auto{…}`,
  `::vfx{type=…}`, a plugin directive by its own name) and drops the bare
  `beat` line; `--ir` prints the lowered records, injected ones included. A
  `newRun` names the `prev.run.*` snapshot it took and lists only the run-tier
  quests it actually reset (with their previous status).
- `lute calendar --script` replays the script's steps before evaluating the
  grid (`--until <step number | label>` stops before that step); `--axis` is
  optional; `quest.<id>.state=…` seeds the quest's status (it was silently
  overwritten); `holds(<fact>)=true,false` asserts / retracts a base fact;
  another `quest.*` path or a derived fact is a usage error; `--where <cel>`
  drops cells where the condition does not hold; a targeted occasion gets one
  column per target its beats name (or per `--target`), not every member of
  its domain. `--json` gains `from`, `pruned`, `anyTarget`; a cell's `note` is
  now `notes` (CSV column too), and the settle moving an axis value is noted.

### Fixed

- **Trace/test seeds read by a beat `when:`** — a `quests:` / `state:` seed of `quest.<id>.state` is admitted when the only read is the scene's frontmatter `when:` (trace already judged it); the refusal for an unread quest now names `quests:` instead of `--state`.
- **Occasion target `members:`** — `target: { prefix, entity, members: [..] }` narrows a domain to a subset of an entity kind; a domain without `members` keeps its `capabilityVersion`.
- **Decider** — a disjunction of numeric comparisons on one path that covers the number line decides true. `cel()` rule guards stay opaque to the decider.
- **`lute tag` no longer renames a shipped component lineId** (ashen N7): an
  untagged component line now takes the code `lute tag` would write into the
  component file (source order, per speaker) before a param-scoped `<match>`
  folds, so a `::use` with a literal argument and one with a `@def` argument
  mint the same code for the same line. **Migration:** a component whose
  untagged lines sit in a non-first `<match>` arm compiles to new `lineId`s
  (the ones `lute tag` persists); re-export localization / voice manifests.
- **Bundle beats are scenario nodes** (lamplight N8, ashen N9): `lute scenario`
  draws every bundle beat as an edgeless entry node `beat(<doc>.<beat>)` (text,
  JSON `kind: "beat"`, DOT `shape=note`); `scenario reach` / `envelope` accept
  its canonical id bare or as `beat:<id>`, and `reach` prints its occasion,
  target and `when`.
- **`lute lore` lists beats** (lamplight N8): bundle beats and scene beats
  appear under their target, labelled `beat` (JSON `kind: "beat"`, `on`), and a
  fact a bundle beat asserts is credited under `beats` (new JSON array), not
  `entries`; `revealedBy` gains `beats`.
- **Match arms narrow their subject** (ashen N3): inside `<when is="x">` (no
  `unset` alternative) the subject is set, and once an arm takes every unset
  value (`is="unset"` with no `test`) later arms and `<otherwise>` see it set —
  no `E-MAYBE-UNSET` for those reads.
- **`W-BEAT-ONCE-RUN-USER` fires only on a defaulted `once`** (ashen N1): an
  authored `once: run` (and an entry's `once="run"`) acknowledges a per-run
  beat; `prev.run.*` counts as run history, not user state. The message adds
  "write `once: run` if it should replay every run".
- **Negation over a stably derived fact** (lamplight F9): the stable-fact set
  closes under rules whose every premise is stable, so `not alibi(crane)` with
  `alibi` derived from seeds nothing removes never holds; an impossible derived
  fact now names the defeating fact (lamplight N16) instead of "no rule".
- **`--wip` grades dead choices, arms, gated lines and `::next`** (lamplight
  N11) like dead entries/beats/objectives.
- **Mis-nested `<entry>` / `<beat>` / `<quest>`** (lamplight F23): one
  `E-UNCLOSED-TAG` "entries cannot nest; `<id>` opened at line N is still
  open", the nested block parsed as a sibling (no cascade); content outside a
  block in a lore/quest document no longer advises a `## ` heading.
- **Stage tracking across partial arms** (seven F7): a character on stage in
  only some arms of a `<match>` / `<branch>` is auto-hidden at the next `::bg`
  (compiled hide) and warned when speaking without a re-show;
  `W-STAGE-ABSENT` names the exit / `::bg` line, and a redundant exit after a
  `::bg` says "this exit does nothing. Move it before the `::bg`, or delete
  it" (lamplight N15).
- **One YAML slip no longer kills guards in other files** (seven F3): when any
  document of a root has an unparseable frontmatter, `check-project` decides
  no fact query impossible.
- **`lute scenario knowledge` names what defeats a negation** (lamplight N10):
  under a negated premise, the rule body is instantiated against the may set
  and each fact that can make it false is listed — `can be defeated by
  alibi(solt, tunnel) ⇐ seen(solt, corridor, tunnel) [entry `mirelaMatch` …],
  away(corridor) [seed]`. Producers are labelled `scene `<key>`` and bundle
  beats by canonical id `beat `<doc>.<beat>``; JSON `assertedBy` labels change
  the same way.
- **`L-EMOTION-DISTRIBUTION` judges each bundle beat alone** (ashen N6): runs
  and streaks are measured per linear unit — a document's shots and quest
  bodies together, each lore `<beat>` on its own — and lore entries are not
  measured, so a bundle's independent beats no longer read as one scene.
- **`lute doctor` checks running `lute-lsp` servers** (seven F27): a new
  `running lute-lsp` line (`runningLanguageServers` in `--json`, Unix) lists
  each running server and fails, advising an editor restart, when one was
  started before its binary was replaced or its binary reports another
  version.
- **`lute doctor` no longer flags a freshly started `lute-lsp` as replaced**
  (macOS): `ps` reports a process's elapsed time as whole seconds counted
  from the second it started, up to a second more than it has run, so a
  server launched within a second of its binary being written read as
  "started before its binary was replaced". The check now allows that second
  of slack.
- **Docs: `pov` is descriptive** (ashen N8): the pages claimed the `pov`
  speaker compiles to a reserved player role; no such IR role exists and
  `pov` does not reach the artifact. The dialogue, frontmatter and cheatsheet
  pages (and `llms-full.txt`) now say so.

### Compatibility

- **Untagged component lines may change `lineId` once.** An untagged line in
  a component now takes the code `lute tag` would write into the component
  file (source order, per speaker) before a param-scoped `<match>` folds, so
  a component whose untagged lines sit in a non-first `<match>` arm compiles
  to new `lineId`s / `voiceKey`s — the ones `lute tag` persists, so they do
  not move again. Re-export localization and voice manifests once; tagged
  lines and lines outside components are unchanged.
- **`::end` in `lute play` ends only its presentation.** It used to stop the
  playthrough; now the presentation (or quest handler) it runs in ends and
  the play goes on with the next step. A script that relied on `::end`
  stopping the play adds a step `- end: true` after that step; later steps
  print as skipped and `--json` lists them under `skipped`.
- **A raised occasion fires the same-named world event.** When a world event
  of the occasion's name is declared, raising the occasion runs every active
  quest's `<on event>` handlers for it, once, before its `on=` objectives are
  judged — in `lute play`, `lute run`, `lute trace` and the runtime contract.
  A handler that ran only on an explicit `event:` now also runs whenever the
  same-named occasion is raised; an engine that fired that event itself at
  the raise stops doing so, or the handler runs twice.
- **Trace and test apply reward credits and objective bodies.** `lute trace`
  and `lute test` now add a reward kind's `credits:` amount and walk
  objective completion bodies as `lute play` does, so a test that expected
  the state without them now fails; update its `expect:`.
- **`E-QUEST-TIER-MIX` rejects mixed-tier quest trees.** A subquest whose
  `tier` differs from its parent's used to check clean and then lock at
  runtime; it is now an error (`check` within one document, `check-project`
  across documents). Give the child its parent's tier.
- **An `on=` objective's `by` waits for its occasion.** Its deadline is judged
  only when the occasion is raised, right after its `done`, so a beat that
  answers the occasion can no longer lose to a deadline judged earlier in the
  same settle.
- **`lute calendar --json`:** a cell's `note` is now `notes` (the CSV column
  too), and the output gains `from`, `pruned` and `anyTarget`.
- **IR shape unchanged.** The version strings move to `0.23.1`;
  [`schemas/lute-ir-0.23.schema.json`](schemas/lute-ir-0.23.schema.json)
  keeps its name and `$id`, and engines, gating on MAJOR, widen nothing.
  `capabilityVersion` moves only for a snapshot whose occasion target domain
  declares `members:`; the tree-sitter grammar is unchanged.

### Known limitations

- `lute play` coverage counts presented documents only, not branch choices or match arms (trace keys arms by source position; play records compiled addresses).

## [0.23.0] - 2026-09-25

**Author overviews and time.**

Once a story is chosen by occasions rather than read top to bottom, a writer
needs to see who can say what, when. This release adds three overviews — a
beat ladder, a calendar over state, and a knowledge map — plus deadlines and
targeted objectives, occasions that compose a routine with an event, several
scene-like beats in one lore file, a checked cast, rewards that credit state,
the previous run's values, and a condition decider that finds contradictions
the checker used to miss. The language and the IR both earn the move (the IR
additively); see
[`docs/proposals/scenario-dsl/0.23.0.md`](docs/proposals/scenario-dsl/0.23.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Changed

- **A sharper condition decider** (dsl 0.23.0 §9): `decide` now reasons per
  path across `&&`/`||` operands (state paths, `$`, component params,
  `holds(P)`/`visited(id)`/`count(P)`). A conjunction whose operands are false
  for every value of one path decides false (`run.n > 5 && run.n < 3`,
  `run.slot == 'a' && run.slot == 'b'`, `x && !x`); a disjunction whose cases
  cover a path's domain decides true. `unset` counts as a value, and an
  ordering that errs on it is neither true nor false, so every verdict is the
  expression's actual value. `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`,
  `E-ARM-DEAD`, `E-OBJECTIVE-UNSATISFIABLE`, `W-BEAT-PRIORITY-TIE` and the
  compile-time fold all benefit. In the may set, a negated rule atom over a
  seed nothing retracts or displaces is false, so `not alibi(crane)` with a
  canon alibi no longer keeps a derived fact possible.

### Added

- **Overviews** (dsl 0.23.0 §1, §11): `project.index.json` beat rows carry
  `when` (expanded) and `title` (additive, omitted when absent). `lute beats
  <dir> [--occasion] [--target] [--json]` prints each occasion/target's beat
  ladder in selection order with priority, `once`, `after`, `when` and the
  `check-project` verdicts (unreachable, shadowed, tied, once-run-user).
  `lute calendar <dir> --axis path=1..7 --axis path=a,b [--occasion]
  [--target] [--script save.play.yaml] [--json|--csv]` evaluates play's own
  eligibility at every cell of the axes' product from the save (quests
  settled per cell): winner, shadowed eligible beats, undecided cells, and
  the beats never eligible anywhere. `lute scenario <dir> knowledge [--for
  <node>]` traces every fact-guarded beat/entry/objective to the atoms it
  queries and their producers through rules (asserting documents, seed
  facts, reserved, or none). The scenario graph notes the
  `completed()`/`active()`/`visited()` references it does not draw because a
  quest declares no `after=`. `lute play` marks an already-read entry
  candidate `read`.
- **Deadlines** (dsl 0.23.0 §2): `<objective by="<condition>">` — a condition
  slot like `done`. The first time `by` holds while the objective is not
  done, the objective fails and is never judged again; a failed required
  objective fails its quest (`failed` rewards, `questFailed`, cascade). `done`
  is judged first, so a deadline never fails a done objective. IR:
  `ObjectiveEntry.by` (additive). `lute trace` records the objective decision
  `failed`; `lute run` / `lute play` print `failed (by)`.
- **Objective targets** (dsl 0.23.0 §2): `<objective on="talk"
  target="npc.maud">` is judged only when the occasion is raised for that
  target, checked like a beat target (`E-BEAT-ATTR`). IR:
  `ObjectiveEntry.target` (additive). `lute trace` / `lute run` raise for a
  target with `occasions: [talk@npc.maud]` / `--occasion talk@npc.maud`; a
  `lute play` step judges objectives for its `target:`.
- **Composing occasions** (dsl 0.23.0 §3): an occasion declared `select:
  sequence` presents every eligible beat in selection order; a scene beat's
  `also: true` rides along after a `select: first` winner and never replaces
  it (IR `BeatIr.also`, additive; `E-BEAT-ATTR` when not a bool, on an entry,
  or on a `select: all` / `sequence` occasion). `W-BEAT-SHADOWED` and
  `W-BEAT-PRIORITY-TIE` ignore `also` beats. `lute play` presents the whole
  list with a quest settle after each beat (`--json`: `presented`, then
  `then`), rejects `pick:` on a `sequence` occasion, and gains the step
  expectation `presented: [ids]`. The `sequence` value changes the
  capability stamp only for snapshots that declare it.
- **Beat bundles** (dsl 0.23.0 §4): a `kind: lore` document may hold
  scene-like `<beat id on target when priority once also title>` blocks with
  a scene body (lines, branches, hubs, matches, directives). A beat's
  canonical id is `<document id>.<beat id>` (the document needs `id:`); it is
  checked like a scene beat (`E-BEAT-ATTR`, `E-OCCASION-UNKNOWN`,
  `E-BEAT-UNREACHABLE` per file and under the fact envelope,
  `W-BEAT-SHADOWED`, `W-BEAT-PRIORITY-TIE`, `W-BEAT-ONCE-RUN-USER`), is
  spent by presentation (`once` defaults to `run`), and `visited('<doc>.<beat>')`
  reads it in any condition (an `after:` still names only scenes and quests).
  A beat id sharing a scene's id is `E-CONN-EPISODE-ID-DUP`. IR: a new `beat`
  command heads each beat's addressing unit in the lore artifact (units in
  source order; an entry's or beat's body segment runs to the next `entry` or
  `beat` record), and `project.index.json` beat rows gain kind `bundle`
  (additive). `lute play` presents bundle beats; `lute trace --beat <id>` and
  `lute run --beat <id>` present one (new `E-TRACE-BEAT`). Tree-sitter, LSP
  (completion, symbols, folding, tokens), `lute tag`/`fix`/`loc`/`context`/
  `doctor`, lint metrics and `lute lore` cover beat bodies.
- **Hub prompts** (dsl 0.23.0 §4): `<hub prompt="…">` attaches the question
  shown with the hub's options (IR `HubCmd.prompt`, additive; an empty prompt
  is `E-BRANCH-PROMPT`). `lute run` / `lute play` print it; LSP completes it.
- **Components with sentences** (dsl 0.23.0 §5): a component `string` param
  may be interpolated (`{{@p}}`); a literal `::use` argument is substituted
  into the line at expansion, so each call site ships its own sentence under
  its own component-scoped `lineId`. Binding such a param to a `@def` ref is
  `E-REF-TYPE` at the argument.
- **Previous run** (dsl 0.23.0 §6): `prev.run.<path>` reads the value every
  declared `run.<path>` had when the previous run ended — typed like its run
  path, maybe-unset before the first run ends, read-only
  (`E-QUEST-RESERVED-WRITE`). `lute play` snapshots it at `newRun`; play
  `state:` seeds and trace mocks may set it.
- **Cast** (dsl 0.23.0 §7): a plugin `cast` export or a schema document's
  `cast: { <id>: { name } }` declares the speakers; once declared, any other
  speaker is `E-CAST-UNKNOWN` with a did-you-mean. `lute context` lists the
  cast and LSP speaker completion offers it.
- **Rewards that credit state** (dsl 0.23.0 §8): `rewardKinds.<kind>.credits`
  names a state path; the IR stamps it on each reward (`RewardEntry.credits`,
  additive), `lute run` / `lute play` add the scalar amount there on grant,
  and a handler `::set` of the same path is `W-REWARD-DOUBLE-CREDIT`.
- **`lute check-project --wip`** (dsl 0.23.0 §10): `E-ENTRY-UNREACHABLE`,
  `E-BEAT-UNREACHABLE` and `E-OBJECTIVE-UNSATISFIABLE` become warnings when
  the guard is dead only because a relation has no producer at all yet (no
  seed, assert, rule, or reserved declaration); a relation that has producers
  but never matches stays an error.

### Fixed

- **`lute run` / `lute play` evaluate structured `isSet` / `has` nodes.** The
  reference runner read the lowered `{isSet}` / `{has}` expression nodes as
  unknown, so a compiled gated line or match arm shaped like
  `isSet(x) && …` never matched. It now evaluates them.
- **`schemas/lute.plugin.json`** lists the `rewardkinds`, `occasions` and
  `cast` exports the loader accepts.

### Compatibility

- **Contradictions the checker used to miss are now errors.** The sharper
  decider reports conditions that can never hold — `run.n > 5 && run.n < 3`,
  `run.slot == 'a' && run.slot == 'b'`, `x && !x`, a negated rule atom over a
  seed nothing retracts — as `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`,
  `E-ARM-DEAD` or `E-OBJECTIVE-UNSATISFIABLE`, and folds provably true/false
  guards at compile. A project that checked clean on 0.22 may redden where a
  guard was already dead; fix the condition. While content is still being
  written, `check-project --wip` downgrades the cases caused by a relation
  with no producer at all.
- **Documents that were already wrong can redden.** Once a project declares
  a cast, an unknown speaker is `E-CAST-UNKNOWN`; a project without a cast
  is unaffected. A reward whose kind declares `credits` beside a handler
  `::set` of the same path warns `W-REWARD-DOUBLE-CREDIT`. Declaring a
  `prev.*` path is `E-STATE-NAMESPACE`.
- **Additive IR.** The version strings move to `0.23.0` and
  `schemas/lute-ir-0.22.schema.json` is renamed to
  [`schemas/lute-ir-0.23.schema.json`](schemas/lute-ir-0.23.schema.json),
  gaining the `cmdBeat` command, `sceneBeat.also`, `objectiveEntry.by` /
  `target`, `cmdHub.prompt`, `rewardEntry.credits`, and `indexBeat` kind
  `bundle` with `when` / `title`. Every new field is optional and every new
  record appears only when the source uses the feature, so artifacts that use
  none of it compile byte-identically apart from the version strings.
  Engines gate on MAJOR, so nothing widens; an engine that predates bundle
  beats rejects a lore artifact carrying a `beat` record (unknown `kind`),
  which is the intended hard error. `prev.run.*` is not an IR state row: the
  engine snapshots `run.*` at run end.
- **`capabilityVersion` moves only when used.** An occasion declared
  `select: sequence`, a non-empty plugin `cast`, and a reward kind with
  `credits` change the capability stamp only for snapshots that declare
  them. The tree-sitter grammar gains `<beat>` in lore documents.

## [0.22.0] - 2026-09-25

**A reference player that can stand in for the engine.**

Three dogfood games each shipped a fake engine inside their content because
`lute play` could not write the state the engine owns, start from a save,
assert anything, or vary decisions per step. This release closes those gaps,
gives run boundaries a lifecycle, lets trace and test apply the project's
Datalog rules, and gives occasion targets a vocabulary. It also carries the
two identity changes 0.21.1 deferred because they alter compiled output. The
language and the IR both earn the move; see
[`docs/proposals/scenario-dsl/0.22.0.md`](docs/proposals/scenario-dsl/0.22.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Changed

- **`lute trace`, `lute test` and `lute play` derive by default** (dsl 0.22.0
  §6, D-B). Trace and test now load the project's seed `facts:` and apply its
  Datalog rules (stratified negation) over the mocked and asserted facts, so a
  rule-derived fact satisfies a guard, `done` or `start` without mocking the
  conclusion, and "derived false because a negated premise holds" is
  testable. A mocked derived atom is still accepted — it is a seed like any
  other. Trace, test and the reference runner (`lute run` / `lute play`) now
  share one evaluator, `lute_trace::datalog`, so they cannot disagree about
  what a project's rules conclude. A rule guard over undecided trace state
  leaves the conclusion unknown and names the state path that would decide it.
- `derive: false` (mock / test / play-script key) and `--no-derive` (`lute
  trace`, `lute test`, `lute play`; the flag wins over the key) restore the
  0.21 model: seeds are not loaded, an unmocked derived atom is unknown, and a
  note names each derived relation read. **Migration:** a test that relied on
  an unmocked derived atom being unknown (exit 3), or on a seeded relation
  reading empty, now sees the derived / seeded answer; pin `derive: false` to
  keep the old verdict.
- **A lore document is testable, so `lute test --coverage` lists an untested
  one** (dsl 0.22.0 §5). It was left out of the untested set because no test
  could target it; a test now names the entries it presents.
- **Breaking: the default `voiceKey` is `{prefix}.{speaker}-{code}`** (dsl
  0.22.0 §11, D-E). The 0.21 default `{speaker}-{code}` repeated across
  documents, so every scene's `@ann{code="0010"}` landed on one voice asset
  (0.21.1 made that `E-DUP-VOICEKEY`). A project that recorded audio against
  the old keys pins `identity: { voiceKey: "{speaker}-{code}" }` in
  `lute.project.yaml` to keep them; a project that already pinned the prefixed
  template (as `lute init` and the example projects do) is unchanged.
- **Breaking: component lines have their own `lineId`/`voiceKey` scope** (dsl
  0.22.0 §11, T1-10). A line expanded from a component is addressed
  `{prefix}.{component}#{n}.{speaker}_{code}`, where `n` counts the host's
  `::use`s of that component (1-based, document order; a nested `::use`
  counts within its enclosing expansion and adds another segment). Each
  expansion back-fills its own untagged codes, so a component line's code no
  longer depends on the host lines before it, and host lines after a `::use`
  keep the codes `lute tag` gives them. Two uses of a tagged component, or a
  component line sharing a code with a host line, now compile clean with
  distinct ids instead of `E-DUP-LINE-CODE`; `lute loc export` emits the same
  ids. **Migration:** re-export localization / voice manifests for documents
  that `::use` components.
- **`W-STAGE-ABSENT` follows paths** (dsl 0.22.0 §12, T1-15). `lute check`
  folds each `<branch>`/`<hub>` choice and `<match>` arm from the stage state
  at the fork and joins the arms at convergence, so an exit in one arm no
  longer warns on a line in its sibling. After the convergence a character is
  on stage only if every arm left them there; one taken off on any arm warns
  when staged again without a re-show. A `::bg` scene change's auto-hide now
  records the hidden characters as exited, so a later line by one of them
  warns (it used to check clean), and a scene change no longer forgets an
  earlier declared exit. `lute compile` uses the same join.
- **Lint defaults stop assuming a linear VN** (dsl 0.22.0 §13, T3-3).
  `L-SHOT-STARTS-WITH-BACKGROUND`, `L-DIALOGUE-RATIO` and
  `L-SCENE-LENGTH-SPREAD` judge only linear scenes: beats (scenes with
  `on:`), components, quests and lore no longer fire them. Rules see the new
  `scene.kind` / `shot.kind` (`scene`, `beat`, `component`, `quest`, `lore`).
  A shot's `firstStagingTag` and `scene.directives` count staging directives
  only — `::accept`, `::use`, `::end`, `::mark`, `::next` are skipped — and
  numbers in lint messages are rounded to two decimals.
- `lute doctor` says "no pinned provider snapshots" instead of calling a
  project with plugins "core-only", and looks for them in the project's
  `catalogDir:` (default `catalog/`) — the directory `check` actually reads.
- `lute init`'s `minimal`/`investigation` manifests drop the `identity:`
  pin: the 0.22.0 default `voiceKey` already carries `{prefix}`.

### Added

- `lute play --explain <atom>` (repeatable): after the play, prints the
  derivation tree of a ground atom — the rule used and each premise's own
  support (seed fact, asserted, or derived in turn), negated premises shown
  `(absent)` — or, when it does not hold, every rule that could conclude it
  with its failing premises (a missing premise explained in turn, a present
  negated premise, a false comparison or guard). `--json` carries the same
  tree.
- **Play-script assertions** (dsl 0.22.0 §4). A play step may carry
  `expect: { winner, offered, notOffered }` (`winner: none` when the occasion
  passed; `offered` is a subset of the eligible beats, order-insensitive) and
  the script a top-level `expect: { exit, quests, state, facts, notFacts,
  transcriptContains, transcriptLacks }` judging the end of the play (`state`
  compares effective values, typed; `facts` after derivation). A miss names
  the step, its `label:` and the actual value, and `lute play` exits 1; an
  unknown `expect:` key is a usage error listing the legal keys.
- **`lute test` runs every `*.play.yaml` that carries an `expect:`**
  alongside `*.test.yaml` (PASS/FAIL lines, `--json` entries with
  `"kind": "play"` and `misses`). A play that halts fails unless its
  top-level `expect:` declares the exit. `--coverage` counts every document a
  play presented (`coverage over N traced path(s) and M play(s)`, JSON
  `coverage.plays`).
- **Scenario-test expectations** (dsl 0.22.0 §5): `transcriptLacks: [...]`,
  `offered: { <choice id>: [opts] }` (the exact set of options the walk
  offered at that branch/hub, across its presentations), and `entry: <id>` /
  `entries: [ids]` for a lore file — the entries are presented in order with
  the read flags set between them, so a repeated id is a re-read. A lore
  test without either is still `E-TEST-LORE`, which now says how to name
  them.
- **Save-history seeds in trace mocks and tests** (dsl 0.22.0 §3):
  `quests: { <id>: unset | active | complete | failed }` and
  `entriesRead: { run: [ids], user: [ids] }` (`entry.<id>.read` /
  `entry.<id>.everRead`). They seed the reserved state paths they spell and
  follow those paths' mock rules — the document must read the path.
- **`owner: engine`** (dsl 0.22.0 §1.2): a `state:` declaration may carry
  `owner: engine`; a content `::set` of that path (or a field under it) is
  the new error `E-ENGINE-OWNED-WRITE`. Reads are unrestricted; any other
  `owner:` value is `E-STATE-DECL`.
- **Run boundaries** (dsl 0.22.0 §7): `<quest tier="run">` (IR
  `QuestCmd.tier: "run"`, omitted for the default `user`; a quest document may
  hold several quests, so the tier rides on each quest record), entry beats
  accept `once="run" | "user"` (IR `EntryCmd.once`, and `ProjectIndex.beats`
  entry rows carry it; absent = repeatable), and the reserved user-tier
  `entry.<id>.everRead` flag is readable everywhere `entry.<id>.read` is
  (`W-ENTRY-REF-UNKNOWN` resolves it too). A bad `tier` is `E-ATTR-TYPE`; a
  bad `once`, or `once` without `on`, is `E-BEAT-ATTR`. New `check-project`
  warning `W-QUEST-HANDLER-DEAD`: `<on event="questFailed">` on a quest with
  no `fail`, no required subquest objective, and no parent quest.
- **Occasion target domains** (dsl 0.22.0 §8): an occasion's `target:` may be
  `{ prefix, entity }`; a scene or entry beat target must then be
  `<prefix>.<member>` of that `entities:` kind (`E-BEAT-ATTR` with a
  did-you-mean; any member of an `open:` kind). `target: true` keeps its
  shape-only meaning and its `capabilityVersion`. `lute_check::occasion_target_ok`
  is the shared rule `lute play` uses for step targets.
- **Beat selection advisories** (dsl 0.22.0 §13, `check-project`):
  `W-BEAT-PRIORITY-TIE` — beats on one `select: first` occasion (targets
  absent or equal) with equal priority whose `when`s are not provably
  exclusive, so file order picks the winner; `W-BEAT-ONCE-RUN-USER` — a
  `once: run` beat whose `when` reads only user-tier state and so replays
  every run. `W-BEAT-SHADOWED` now treats an entry with `once` as spendable.
- **`engine:` play steps** (dsl 0.22.0 §1.1, D-A): `- engine: { state: {…},
  facts: […], retract: […] }` writes what the engine owns — declared state
  as a literal or `{ add: <number> }`, ground atoms of any declared base
  relation, reserved ones included — checked against the declared types,
  entity members and arities before anything plays (exit 2). The step
  presents nothing and raises no occasion; the quest lifecycle settles after
  it, so a write can complete or fail a quest on the spot. `newRun` also
  takes `{ state, facts }`, applied after the reset as the new run's seed,
  and a new run now settles the quest lifecycle too. The fake-engine scenes,
  occasions and plugins the dogfood games shipped are no longer needed.
- **Per-step `choose:`** (dsl 0.22.0 §2): an occasion step's own map
  replaces the script's `choose:` key by key for that presentation; a
  step-local decision list starts fresh and leaves the script-wide list's
  consumption untouched.
- **Play scripts start from a save** (dsl 0.22.0 §3): top-level `visited:`,
  `presented: { run, user }` (spent `once` beats; presented scenes count as
  visited), `quests:` and `entriesRead: { run, user }`. An id the project
  does not declare is a usage error with a did-you-mean, and a `state:` seed
  that does not fit its path's declared type now is one too.
- **Run boundaries in `lute play`** (dsl 0.22.0 §7): a `newRun` returns
  `<quest tier="run">` quests to `unset` with their objectives undone; a
  first read sets `entry.<id>.everRead`, which no `newRun` resets; an entry
  beat with `once="run"` / `once="user"` is not eligible once its read flag /
  `everRead` is set.
- **`event:` play steps** (dsl 0.22.0 §9) fire a declared world event: the
  `<on event>` handlers of active quests run, as trace `events:` fires them.
  An `event:` naming an occasion, or an `occasion:` naming a world event,
  says which step key to use.
- **`pick: none`** (dsl 0.22.0 §10) closes a `select: all` list: nothing is
  presented or spent, and `on=` objectives are still judged.
- Play steps take `label:` (printed in the step header, carried in `--json`)
  and `repeat: <n>` (the step runs `n` times; each repetition is its own
  step record). A play step's target must lie in its occasion's target
  domain (dsl 0.22.0 §8) — a usage error with a did-you-mean otherwise.
- **`lute init --template beats`** (dsl 0.22.0 §13, T3-1): an occasions
  plugin, `defaults: { luteVersion, uses }`, a `world.schema.yaml` with an
  `owner: engine` clock and shorthand defs, beats with `id:`, a quest, entry
  beats, a play script with `engine:` steps and `expect:`, and scenario tests
  — `check-project`, `test` and `play` pass as scaffolded.
- **`lute new scene <name> --on <occasion> [--target <target>]`** writes a
  beat, checking the occasion and target against the project (a did-you-mean
  and exit 2 otherwise, leaving no file). Every `lute new` document now has an
  `id:` (scenes no longer get the `character`/`season`/`episode` triple),
  omits what the manifest's `defaults:` supplies, lands under the enclosing
  project's root, and `/` in a name nests it in a subfolder. Outside a
  project `lute new` says so, and refuses `--on`.
- **`lute context`** (T3-4) adds `defs` (type, params, body), relation
  `tier` and `reserved`, `owner: engine` on state paths, component
  signatures, occasion target domains and descriptions, the built-in
  directives (`::set`, `::assert`, `::retract`, `::accept`, `::use`), and
  every scene, quest and entry id in the `--project` (JSON: `defs`,
  `builtinDirectives`, `ids`).
- **`lute doctor`** (T3-5) reports the active plugins, every declared
  occasion with the number of beats answering it, the play scripts and
  scenario tests, and whether the `lute-lsp` on `PATH` is this toolchain's
  version. `lute-lsp --version` prints `lute-lsp <version>`.
- **`lute tag` / `lute fix` accept a directory** (T3-14): every `.lute` file
  under it, recursively and in sorted order, each line naming its file, then
  a summary. A refused or unreadable file is reported and the walk goes on;
  the exit code is the worst outcome.
- `lute-lsp` completes and documents on hover `<quest tier>` and
  `<entry once>`.

### Compatibility

- **Compiled identity changes (breaking).** Two changes alter the
  `lineId` / `voiceKey` strings `lute compile` emits: the default `voiceKey`
  is now `{prefix}.{speaker}-{code}`, and a line expanded from a component is
  addressed `{prefix}.{component}#{n}.{speaker}_{code}`. A project on the
  0.21 default `voiceKey` that recorded audio against it pins
  `identity: { voiceKey: "{speaker}-{code}" }` in `lute.project.yaml` to keep
  its keys; a project that already pinned `{prefix}.{speaker}-{code}` sees no
  voice-key change. Documents that `::use` components get new component-line
  ids either way — re-export localization and voice manifests for them. There
  is no `lute fix` rewrite: the pin is the migration.
- **Trace and test results can change.** Derivation is on by default, so a
  test that relied on an unmocked derived atom being unknown (exit 3), or on a
  seeded relation reading empty, now sees the derived / seeded answer. Pin
  `derive: false` in the test or mock, or pass `--no-derive`, to keep the
  0.21 verdict. `W-STAGE-ABSENT` follows paths, so a scene that warned on a
  sibling arm checks clean, and a line after a `::bg` auto-hide now warns;
  lint defaults stop firing linear-VN rules on beats, components, quests and
  lore.
- **Quest persistence is unchanged.** `<quest tier>` defaults to `user`, so
  every existing quest keeps its status across runs; only a quest that opts
  into `tier="run"` resets. An entry without `once` stays repeatable, and an
  occasion with `target: true` keeps its shape-only meaning and its
  `capabilityVersion`.
- **Documents that were already wrong can redden.** `E-ENGINE-OWNED-WRITE`
  fires only on a new `owner: engine` declaration; a beat target outside an
  occasion's new target domain is `E-BEAT-ATTR`; `check-project` adds the
  warnings `W-QUEST-HANDLER-DEAD`, `W-BEAT-PRIORITY-TIE` and
  `W-BEAT-ONCE-RUN-USER`. A play script whose `state:` seed does not fit its
  declared type, or that names an unknown id, is now a usage error (exit 2).
- **Additive IR.** The version strings move to `0.22.0` and
  `schemas/lute-ir-0.21.schema.json` is renamed to
  [`schemas/lute-ir-0.22.schema.json`](schemas/lute-ir-0.22.schema.json),
  gaining the optional `cmdQuest.tier` and `entryCmd.once` (also on the
  `ProjectIndex.beats` entry rows). Engines gate on MAJOR, so nothing widens;
  an engine without run tiers ignores both fields. `capabilityVersion` moves
  only for a project whose occasions declare a target domain; the tree-sitter
  grammar is unchanged.

## [0.21.1] - 2026-09-25

**No silent wrong answers.**

A patch on the `0.21` line. An audit of the toolchain found checks that
accepted a defect and then shipped the wrong thing — a passing test that never
reached its expectations, a `{{@def}}` an engine could only print as a marker,
a directive attribute dropped from the IR, two scenes' lines landing on one
voice asset. Each now reports the defect where it is made. No syntax is added;
the language's static semantics tighten, and the IR gains one optional field.
See [`docs/versioning.md`](docs/versioning.md) for what each axis earned.

### Added

- **New `E-DUP-VOICEKEY`.** The default `voiceKey` template
  `{speaker}-{code}` has no `{prefix}`, so lines of different scenes landed on
  one voice asset without a word. `check-project` and `compile --all` now
  refuse a key carried by lines with different text, naming each line; set
  `identity.voiceKey: "{prefix}.{speaker}-{code}"` (the default is unchanged
  until 0.22.0). `lute init` projects, the `docs/examples` projects and the
  first-scene tutorial now pin it (T1-9).
- **New `E-CAPABILITY-MISMATCH` at `check-project`.** A project whose documents
  resolve two capability snapshots passed `check-project` and was then refused
  by `compile --all` and `play`; `check-project` now runs the same
  single-snapshot gate, with the same message. `docs/examples/showcase` was
  such a project; its three scenes now share one set of scene-local options
  (T1-11).
- **IR — `expr` on a `ref` placeholder.** The referenced def body, inlined as
  a `{raw, expr}` CEL pair, so an engine renders `{{@def}}` by evaluating it
  like any other CEL slot (see *A `{{@def}}` renders its value* below).
  Optional in [`schemas/lute-ir-0.21.schema.json`](schemas/lute-ir-0.21.schema.json),
  which keeps its name and `$id`.

### Fixed

- **`lute test`: an incomplete trace fails.** A walk halted by an unknown guard
  reported `{"exit":"incomplete","passed":true}` whenever the test did not
  mention `exit:` — the expectations it never reached were never checked. It now
  fails unless the test opts in with `expect: { exit: incomplete }` (T1-13).
- **`lute test`: a lore document cannot be a test subject.** A test naming a
  lore file walked nothing and passed; it now fails with `E-TEST-LORE` and points
  at `lute trace <file> --entry <id>` (T1-13).
- **`lute test --coverage` measures the project.** The untested set was taken
  from the directory the tests live in, so `lute test tests --coverage` always
  said every document was tested. It now walks `--project`, else the nearest
  `lute.project.yaml` (T1-13).
- **`lute test`: `expect.state` compares the effective value.** A path the walk
  never wrote read "never written" even when its declared `default:` or the
  test's own `state:` seed was exactly the expected value. The comparison now
  uses trace's own read order: write, then seed, then default (T2-5).
- **`lute trace` names a beat scene whose `when` does not hold.** A scene traced
  under state where its frontmatter `when:` is false (or undecided) read as a
  plain `complete`; the trace now opens with a `beat \`when\`` note, and `lute
  test` shows it on the test line (T1-13).
- **`lute trace`: a choice forced past an unknown guard counts as unresolved.**
  It was only a `(forced)` suffix; the summary now counts it and names the atoms
  that would decide it, and `--json` carries `forcedUnknown`. The exit code is
  unchanged (T1-13).
- **`W-TRACE-MOCK-UNPRODUCIBLE` consults the project.** It judged a mocked fact
  against the traced document's own asserts, so every clue a sibling scene
  establishes was "not producible". With `--project`, or a `lute.project.yaml`
  above the file, it now uses the project's reachability-gated producer set;
  without a project the note says it judged this document only (T1-14).
- **`lute test` failures say why the walk stopped.** The unresolved guards and
  the `state:`/`facts:` entries that would decide them are printed, and `--json`
  carries `unresolved` (with `atoms` and `supply`) per test (T3-11).
- **No panic on a closed pipe.** `lute scenario`, `lute test`, `lute lore` and
  `lute trace` write their report once through the same EPIPE-safe path
  `compile` uses, so `… | head` exits instead of panicking (T3-15).
- **`lute play`: a `quest.<id>.state` seed is the quest's status.** It landed in
  state only, so the start settle re-registered the quest as `unset` and every
  quest-gated beat read the wrong status. The seed now registers the quest with
  that lifecycle status; an undeclared quest id or a value outside
  `unset|active|complete|failed` is a usage error (exit 2) (T1-1).
- **`lute play`: `::end` settles the step first.** A beat that ended the
  playthrough skipped the quest advance and the occasion's `<objective on>`
  judging, and still exited 0. The step's lifecycle now settles, then the walk
  stops (T1-2).
- **`lute play` holds the script to what is offered.** Forcing a spent `once`
  hub option was silently skipped; it now halts with `E-TRACE-CHOICE`, as `lute
  trace` does. A `choose:` list of two or more decisions for a `<branch>` is
  consumed one per presentation, in order, across the playthrough (also in
  `lute run`) instead of repeating its first entry; running out halts
  incomplete and says so. A single decision still answers every presentation
  (T1-8).
- **`lute play`: an ineligible `choose:` exits 1**, like an ineligible `pick:`;
  it was 2, the usage-error code (T3-19).
- **`lute play`: a `target: true` occasion raised without `target:` is a usage
  error** (exit 2); it played `(no candidates)` at exit 0 (T2-9).
- **A component `{{@param}}` renders the bound argument.** Every `::use`
  expansion shipped the same `"Outside: {{@weather}}."` with a `ref`
  placeholder naming a param that no longer exists, so `lute run`/`lute play`
  printed the marker. The param's literal is now substituted into each
  expansion's text (`Outside: grey.` / `Outside: still.`); a param bound to a
  caller-side def stays a `ref` placeholder naming that def (T1-3).
- **A `{{@def}}` renders its value.** The artifact has no defs table, so an
  engine could only print `{{@twice}}`. A `ref` placeholder now carries the def
  body inlined as `expr` (`{raw, expr}`; optional in
  `schemas/lute-ir-0.21.schema.json`), and `lute run`/`lute play` evaluate it.
  A def that cannot be inlined into one expression (an expansion cycle, a body
  that reads `$`) is the new error `E-INTERP-DEF` at `lute check` (T1-3).
- **A def in a directive attribute is never dropped.** `::camera{zoom=@closeUp}`
  compiled with no `zoom` at all, and `::bg{time=@slotNow}` shipped the CEL
  source `"(run.slot)"` as the time. A def that folds to a constant is now
  written as that literal (`zoom: 1.3`) and checked like an authored one
  (`E-BAD-ENUM`, `E-ATTR-TYPE`); a state-dependent def — directly, or as a
  `::use` arg the component puts into an attribute — is the new error
  `E-ATTR-DEF-DYNAMIC` (T1-4).
- **`lute check`: `quest.<id>.state` is an always-assigned lifecycle enum.**
  Every read was `E-MAYBE-UNSET` with a message about `::set`, and `== 'unset'`
  added `E-UNSET-LITERAL` — while play, trace and `lute test` all treat `unset`
  as the state before activation. Reads and `== 'unset'` are now clean, `<when
  is="unset">` names that member (and compiles to `== "unset"`, which fires;
  `!isSet(…)` never did), `$ == null` no longer counts as covering it, and
  `isSet(quest.<id>.state)` — always true — is the new warning
  `W-QUEST-STATE-ISSET` (T1-1).
- **`lute check`: a `@def` argument to an enum component param is checked.** A
  string def was compatible with every enum, so `depth=@pick` with a body that
  could produce a non-member passed and matched no arm. Every value the body
  can produce must be a member, or it is `E-COMPONENT-ARG` (T1-5).
- **`lute check`: def bodies get the CEL profile gate.** The `@name` use site
  is exempt as a macro and nothing looked at the body, so `%`, `size()` and
  even unparseable CEL passed. A body is now `E-CEL-PROFILE`/`E-CEL-PARSE` at
  its own key; the `E-DEF-DECL` hint writes `type: <bool|number|enum>` instead
  of guessing `bool` (T1-6).
- **`<quest>`, `<objective>` and `<on>` close their attributes.** An invented
  or misspelt key (`fial=`, `optinal`, `target=`) was accepted and dropped from
  the IR; it is now `E-UNKNOWN-ATTR` with a did-you-mean, as every other logic
  tag already was. LSP completion offers exactly the permitted keys (T1-7).
- **Attribute values: `\"` is a quote, `'…'` is an error.** A `\"` inside a
  quoted value kept its backslash in the label; it is now stored as `"` (other
  escapes still reach CEL untouched). A single-quoted value (`label='"Hi."'`)
  silently kept its quotes; it is now `E-ATTR-QUOTE` (T1-16).
- **Component lines keep distinct `lineId`s after expansion.** A tagged line
  in a component `::use`d twice, or sharing its `(speaker, code)` with a line
  of the host scene (typically after `lute tag`), compiled to two records with
  one `lineId`/`voiceKey`. The expanded stream is now checked: `E-DUP-LINE-CODE`
  at the `::use`, from `lute check`, `check-project` and every compile (T1-10).
- **`<when is=… test=…>` covers only what both prove.** An arm with both was
  counted as covering its whole `is=` set, so a later arm on the same member
  drew a false `W-OVERLAP-ARMS`, and a match missing members was taken as
  exhaustive (no `E-NONEXHAUSTIVE`) while the runtime matched no arm. A guard
  the checker cannot decide now covers nothing (T1-12).
- **`W-LUTE-VERSION-STALE` compares versions as numbers** and, for a stamp
  newer than the toolchain, says to upgrade the toolchain rather than restamp
  (T3-6).
- **Component-body diagnostics point at the `::use`.** They were anchored at
  the host's frontmatter (1:1) with the component's absolute path; they now
  land on the first `::use` that brings the body in and name the component
  relative to the project. `{{p}}` for a declared param now says to write
  `{{@p}}`, and a line whose whole text is `@name` for a def or param draws the
  new warning `W-TEXT-LOOKS-LIKE-REF` (it ships the literal text) (T3-7).
- **`E-META-PARSE` stops that document's checks.** An unparseable frontmatter
  was followed by a dozen errors from checking the body against an empty
  environment, and by `E-CONN-UNKNOWN-NODE` in every other file that named the
  broken scene (T3-8).

### Changed

- **`check-project` runs the compile.** Every document that checks clean is
  compiled under its project's `identity:`, so compile-stage errors (such as
  `E-DUP-LINE-CODE` and `E-DUP-VOICEKEY`) fail `check-project` instead of only
  `compile` (T1-9, T1-10).
- **`lute check <file>` uses the project the file is in.** Without `--project`
  it checked the file with no manifest — no `defaults: uses:`, no profile — and
  reported `E-UNDECLARED`/`E-DOMAIN-UNKNOWN` for paths the project declares. It
  now applies the nearest `lute.project.yaml` and says so on stderr (`note:
  using project …`) (T3-9).
- **`lute scenario envelope`: Possible lists only what is not Guaranteed.**
  Possible is a superset of Guaranteed, so every guaranteed path was printed
  twice. The quest envelope's separate `Possible \ Guaranteed` inventory is now
  that same Possible table (T3-15).
- **`lute play` transcripts read like the source.** Lines keep their delivery
  (`@wren{mono}:`, `as=`), a `when=`-guarded line shows as itself or as
  `skip @maud "…" — when: false` instead of `match -> arm 1`/`otherwise`,
  compiler-injected staging (preloads, pose resets, `::bg` auto-hides) is left
  out, and menus mark options not offered (`piano✗`, `table(spent)`). `--json`
  line records carry `role`, `lineId`, `voiceKey`, `as` and `emotion`, and
  menu records `spent`/`ineligible`. `lute run`'s transcript (the conformance
  contract) is unchanged (T1-8, T3-2).

### Compatibility

- **Multi-scene projects on the default `voiceKey` template now fail.** The
  default `{speaker}-{code}` carries no scene prefix and every scene numbers
  its lines independently, so in practice every project with two or more
  scenes gets `E-DUP-VOICEKEY` from `check-project` and `compile --all` until
  its `lute.project.yaml` pins
  `identity: { voiceKey: "{prefix}.{speaker}-{code}" }`. Pinning renames
  every voice asset key, so re-export voice-asset manifests afterwards.
- **Incomplete traces fail tests.** A `lute test` case whose walk halts on a
  guard it cannot decide used to pass whenever it did not mention `exit:`; it
  now fails and names the `state:` / `facts:` entries that would decide the
  guard. Supply them, or write `expect: { exit: incomplete }` when stopping
  there is the point of the test. A test whose `file:` is a lore document
  fails with `E-TEST-LORE`.
- **Documents that were already wrong can redden.** New errors
  (`E-ATTR-DEF-DYNAMIC`, `E-INTERP-DEF`, `E-ATTR-QUOTE`), attribute closure on
  `<quest>` / `<objective>` / `<on>` (`E-UNKNOWN-ATTR`), the CEL profile gate
  on def bodies (`E-CEL-PROFILE` / `E-CEL-PARSE`), enum checking of `@def`
  component args (`E-COMPONENT-ARG`), `E-DUP-LINE-CODE` over expanded
  components, `E-NONEXHAUSTIVE` for an `is=` + `test=` arm that was wrongly
  counted as covering, and `check-project`'s compile and single-snapshot gate
  (`E-CAPABILITY-MISMATCH`) each fire only where the artifact was already
  wrong. `quest.<id>.state` reads lose their false `E-MAYBE-UNSET` /
  `E-UNSET-LITERAL`.
- **Additive IR.** The version strings move to `0.21.1`; the one shape change
  is the optional `placeholder.expr`, so `0.21.0` artifacts stay valid against
  the `0.21` schema and engines, gating on MAJOR, widen nothing. Some
  artifacts change content: a `<when is="unset">` arm on a quest state
  compiles to `== "unset"`, a `\"` in an attribute value is stored as `"`, and
  a def in a directive attribute that folds to a constant is written as that
  literal. `capabilityVersion` does not move; the tree-sitter grammar is
  unchanged.

## [0.21.0] - 2026-09-24

**Beats and occasions: story selection without a clock.**

The `0.11.0` schedule layer modelled one kind of game — a visual novel on a
day clock, every scene a placement at a tick on a lane. Most narrative games
do not advance by clock: at some moment (a hub visit, entering a room,
talking to an NPC, a new day, the start of a run) they pick one of the story
pieces whose conditions hold. An audit of the schedule found the rest: quest
progress could not gate a placement (`completed()`/`active()` read an empty
set), placement order was a second source of truth beside `after:`, and
recurring events were forbidden. `0.21.0` replaces it. The engine raises
**occasions**; a scene or lore entry becomes a **beat** by naming the occasion
it answers, with a `when`, a `priority`, and a repetition policy; Lute defines
which beats are eligible and which one wins. Spec:
[`docs/proposals/scenario-dsl/0.21.0.md`](docs/proposals/scenario-dsl/0.21.0.md);
engine contract:
[`docs/runtime/beats-and-occasions.md`](docs/runtime/beats-and-occasions.md).

### Added

- **Plugins — `occasions:` export** — a plugin manifest may declare the
  occasions its engine raises: `select: first` (default — present the single
  winner) or `select: all` (offer every eligible beat and let the player
  pick), `target: true` for an occasion raised *for* something (`talk` →
  `npc.achilles`), and an optional `description`. Folded into the capability
  snapshot as a guarded, sorted section (the `rewardKinds` precedent), so a
  project without it keeps its `capabilityVersion`. With no declaring plugin,
  occasion names are shape-only.
- **Language — scene beats** — scene frontmatter `on:` (the occasion),
  `target:` (a dotted id), `when:` (a CEL condition over `run`/`user`/`app`
  state, `quest.*`, `entry.<id>.read`, and fact queries — never the scene's
  own `scene.*`), `priority:` (integer, default `0`, higher wins), and
  `once: run | user | false` (default `run`). A beat is eligible when its
  `after:` and `when` both hold and its `once` is unspent. `when` joins the
  CEL-slot registry like a quest `start`. The keys are scene-only and never
  defaultable.
- **Language — entry beats** — `<entry on="…" priority="…">` beside the
  existing `when` / `target`. Entries have no `once`; an entry heard once
  guards on its own `entry.<id>.read`.
- **Diagnostics** — `E-BEAT-ATTR` (a malformed `on` / `target` / `priority` /
  `once`, beat keys without `on`, or a `target` on an untargeted occasion),
  `E-OCCASION-UNKNOWN` (an occasion no resolved plugin declares, once some
  plugin declares occasions), `E-BEAT-UNREACHABLE` (a scene beat whose `when`
  provably never holds — scalar conditions per file, fact conditions in
  `check-project` through the `0.20.0` fact envelope), and the `check-project`
  warning `W-BEAT-SHADOWED` (a `select: first` beat that can never win because
  an earlier-ordered, always-eligible, never-spent beat on the same occasion
  and target always does).
- **IR** — `SceneMeta.beat` (`{on, target?, when?, priority, once}`, `once` as
  `"run"` / `"user"` / `"none"`), `EntryCmd.on` / `priority`, and
  `ProjectIndex.beats` (every scene and entry beat in selection-tiebreak
  order: document path, then declaration order). All omitted when absent.
- **CLI — `lute play <dir> --script <play.yaml> [--json]`** — rebuilt on
  occasions. A script lists `steps:` (`{occasion, target?, pick?}` and
  `{newRun: true}` boundaries) plus `state:` / `facts:` seeds and `choose:`
  branch decisions in the trace-mock grammar. Each step prints every candidate
  beat with its verdict (`once`, `after:`, `when`), presents the winner — or
  the step's `pick` on a `select: all` occasion — through the same reference
  runner as `lute run`, and then advances every quest lifecycle. `newRun`
  resets `run.*` state, run-tier facts, and `once: run` spending. Exit `0`
  complete, `1` a failed project compile or an ineligible `pick`, `2` a usage
  error (malformed script, unknown occasion), `3` an incomplete walk.
- **LSP** — hover and completion for the `<entry>` `on=` / `priority=`
  attributes.
- **Docs** — [`docs/runtime/beats-and-occasions.md`](docs/runtime/beats-and-occasions.md)
  (candidates, eligibility, spending, selection order, presentation), and the
  website's *Beats* language page and *Playing a story* tooling page.
- **Language — quests meet scenes and occasions (dsl 0.21.0 §7a)** —
  `visited('<scene id>')` is legal in every condition slot (quest `start` /
  `fail`, objective `done`, beat and entry `when`, content-line and branch
  `when=`), so a scene advances a quest by being played, without a relay
  flag; an unknown id is `E-CONN-UNKNOWN-NODE`. `<objective on="<occasion>">`
  judges the objective only when that occasion is raised — the missing
  end-of-run check point. `::accept{quest="<id>"}` accepts an accept-driven
  quest from a scene; `E-ACCEPT-TARGET` rejects a missing, unknown, or
  `start=`-gated target. IR: `ObjectiveEntry.on`, command
  `{kind: "accept", quest}`; `visited()` slots carry `raw` only, like `holds()`.
- **CLI** — `lute trace` / `lute run --occasion <name>` (repeatable) and the
  mock / test keys `visited:` and `occasions:`; `lute test`
  `expect.quests: {<id>: unset | active | complete | failed}`; `lute play`
  occasion steps judge `on=` objectives, and an occasion only objectives
  reference is a legal step.
- **Language — def shorthand and `E-DEF-DECL` (dsl 0.21.0 §7b)** — a def
  may be written as its CEL body alone, `vesnaHasPaper: "holds(knows(vesna,
  manifest))"`, and the long form's `type:` is optional: an absent type is
  inferred from the body by the same closed procedure that types a `::set`
  (a body it cannot type asks for the long form; an explicit type must agree
  with the body's). `E-DEF-DECL` rejects every other shape — a non-string,
  non-mapping value, a mapping without a string `cel:`, a bad `type:`, an
  unknown key, `params:` without `type:` — inline and in an imported schema.
  This closes a gate hole: a def whose body `check` could not see (a bare
  string, before) passed `check` and then failed `compile`/`trace` with
  `E-COMPILE-EXPAND … (gate should have caught this)`. The published
  `lute.schema.json` def shape now matches (`cel` required; `min`/`max`/
  `values` — never read by the checker — removed).

### Changed

- **Start-less quests are accept-driven in `lute run` / `lute play`** — an
  unreferenced quest with no `start` stays `unset` until a mock `accepts:`
  entry or an `accept` record names it; it used to activate at walk start.
  This matches `lute trace` (dsl 0.4.0 §4.4) and `quest-lifecycle.md`, whose
  contradictory "activates at the start of the walk" line is corrected.
  Subquest children are unchanged.
- **`lute scenario` lists bare quests** — a quest without `after=` appears as
  `unanchored` (text, `roots[].unanchored` in JSON, a dashed DOT node) instead
  of vanishing; `scenario reach` on one reads `Unanchored — …` (token
  `unanchored`, formerly `reachable`).
- **`lute run` transcript** gains `accept` and `occasion` records; `lute trace`
  JSON gains the `accept` step.
- **`lute play` drives quest lifecycles** — after each presentation it
  advances every quest exactly as `lute run` does for a quest artifact
  (resuming carried status rather than restarting), so a later `when` over
  `quest.*` and an `after: completed(…)` see real progress. The
  schedule-era player never advanced quests.
- **`lute context`** lists the project's declared occasions (`occasions` in
  `--json`, with each occasion's `select` and `target`) beside the other
  vocabulary.

### Removed

- **`schedule.yaml`** — the project-file layer, its loader and route-space
  sweep, and the clock / lane / placement model. No project used it; a
  day-clock story is a `dayStart` occasion plus `when` over `run.day`.
- **Schedule-driven `lute play`** — the `--state`, `--fact`, `--choose`,
  `--auto`, `--lanes`, `--steps`, and `--coverage` flags go with it; seeds and
  decisions now live in the play script. The reference runner's `--auto first`
  hub fallback (auto-selecting the first eligible option once a scripted
  sequence ran out) is gone: an unscripted hub halts the walk incomplete.
- **Every schedule diagnostic** — `E-SCHED-AT-PARSE`, `E-SCHED-BUCKET-DUP`,
  `E-SCHED-CLOCK-OVERFLOW`, `E-SCHED-CLOCK-STRUCTURE`,
  `E-SCHED-CURSOR-DYNAMIC`, `E-SCHED-DOC-MISSING`, `E-SCHED-DOC-PATH`,
  `E-SCHED-EVENT-DUP`, `E-SCHED-GUARD-PARSE`, `E-SCHED-LANE-UNKNOWN`,
  `E-SCHED-SIZE-INVALID`, `E-SCHED-USER-OVERLAP`, `E-SCHED-VARIANT-AMBIG`,
  `E-SCHED-VARIANT-FORM`, `E-SCHED-VARIANT-GAP`, `W-SCHED-DOC-UNPLACED`,
  `W-SCHED-IDLE`, and `W-SCHED-ROUTESPACE-CAP`.
- **`docs/schedule-and-play.md`** and the website's *Schedule & play* pages
  (en + ko), replaced by *Playing a story*.

### Compatibility

- Additive IR: scene and lore artifacts without beats compile
  byte-identically apart from the version strings; the IR restamps to
  `0.21.0` and the schema file renames to
  [`schemas/lute-ir-0.21.schema.json`](schemas/lute-ir-0.21.schema.json),
  gaining `sceneBeat`, the entry fields, and the `indexBeat` row. Engines gate
  on MAJOR, so nothing widens; an engine without beat support ignores the new
  fields and reaches every scene by explicit flow.
- A project carrying a `schedule.yaml` keeps checking and compiling (the file
  is simply no longer read); scripts invoking `lute play` with the removed
  flags must move to `--script`.
- `capabilityVersion` moves only for a project that installs an
  `occasions:`-declaring plugin; the tree-sitter grammar is unchanged.
- `E-DEF-DECL` can redden a def that compiled before only when that def was
  already wrong: an unknown key was silently ignored, and a `type:` that
  disagrees with its body's decidable type (`type: string` over the CEL
  number `0010`) mistyped every `@ref` to it. A type-less long-form def, which
  used to be unchecked at its `@ref` sites, is now typed by inference, so an
  existing misuse of it can surface as `E-REF-TYPE`.

## [0.20.0] - 2026-09-24

> **First published toolchain since `0.17.2`.** The `0.18.0` and `0.19.0`
> entries below were never published as packages; their language and IR
> changes (range patterns in `<when is>`, `W-WHEN-TEST-LITERAL`, lore entries,
> document bundles) ship in this release together with fact envelopes.

**Fact envelopes: `check-project` decides relational guards.**

An author who writes a line that presumes knowledge guards it —
`@eris{when="holds(knows(player, project_lumen))"}` — and until now the checker
never looked at that guard. A relational query was always *undecided*: the
producibility walk asked only whether a relation **name** was asserted
somewhere, and only for quest `start`/`fail`, objective `done`, and entry
`when`. A line guard on a fact nothing ever asserts — a typo'd argument, a cut
scene, a lore entry never written — checked clean, while
`W-UNPROVEN-RELATIONAL` marked every other relational gate "not proven", noise
on exactly the guards that were fine. `0.20.0` computes, for every
`holds(…)` / `count(…)` in every guard slot, whether the queried facts are
**impossible**, **guaranteed**, or **possible** there. Spec:
[`docs/proposals/scenario-dsl/0.20.0.md`](docs/proposals/scenario-dsl/0.20.0.md).

### Added

- **Language — the may set** — a project-wide, argument-level
  over-approximation of every ground fact that can be live: `facts:` seeds,
  every `::assert` in a document not proven unreachable (scenes, quest bodies,
  lore entries), every fact of a reserved relation, and the rules' closure
  over them (negated atoms and rule-body CEL guards read as satisfiable).
- **Language — the must set** — a path-sensitive under-approximation of the
  facts live on every route to a slot: a forward must-dataflow within each
  document (branch/match joins intersect, a hub is a greatest fixpoint,
  `::end`/`::next` route their set), propagated across the `after:` graph
  with the scalar envelope's recursion (`visited(A)` contributes `A`'s exit,
  `&&` unions, `||` intersects). Only monotone facts cross a document
  boundary — no matching `::retract`, no `key:` conflict, not reserved or
  engine-open, not `tier: scene`/`tier: quest`. Guards are assumptions inside
  their regions; quest and entry bodies start from the seeds plus their own
  guards. `count(P)` is decided over the interval `[|Must ∩ P|, |May ∩ P|]`.
- **Checker — verdict plumbing** — the verdict is substituted into `decide`,
  so every slot that already reports a provably dead or always-true condition
  now does so for relational ones, with messages that name the fact and the
  reason (the assert site, seed, rule, or enclosing guard). Project-level
  only: single-file `lute check` leaves relational queries undecided.
- **Diagnostics — `E-ENTRY-UNREACHABLE`** — a lore entry `when` that provably
  never holds (the one guard slot without a dead-code diagnostic); it also
  fires for a scalar-decidable `when` in single-file `lute check`.
- **Diagnostics — `W-FACT-GUARANTEED`** — a relational query inside a guard
  (line `when=`, `<choice when>`, `<when test>`, entry `when`) that holds on
  every route to it: the condition is redundant. Quest `start`/`fail` and
  objective `done` are predicates, not guards, and are not flagged.
- **CLI — `lute scenario <dir> envelope <node>`** prints a *Guaranteed facts*
  table (each fact with what establishes it) beside the scalar tables;
  `--format json` carries it as `envelope.guaranteedFacts`
  (`[{fact, establishedBy}]`).

### Changed

- **Dead relational guards reuse each slot's code** — `E-ARM-DEAD`
  (`<when test>`, `<choice when>`, content-line `when=`, `::next` guard),
  `E-OBJECTIVE-UNSATISFIABLE`, `E-QUEST-UNREACHABLE`, `W-OBJECTIVE-HIDDEN`.
- **Examples** — the four guards `W-FACT-GUARANTEED` found in `docs/examples`
  are removed (haven `purser.lute` `listTheMass`; investigation
  `crime-scene.lute`'s derived `points(blake)` line and `interview.lute`'s two
  hub choices), with their tests, READMEs, and the website tutorial updated;
  the `lute init --template investigation` scaffold drops the same two
  guards. `check-project docs/examples` reports no warnings.

### Removed

- **`W-UNPROVEN-RELATIONAL`** — its premise, that relational gates are
  unanalyzable, no longer holds, and a *possible* gate is the normal state of
  a guard. Following the `W-INJECT-CONFLICT` (0.10.0) precedent the code
  leaves the deny registry: `--deny W-UNPROVEN-RELATIONAL` is a usage error
  (exit `2`).

### Compatibility

- No grammar or IR change: every 0.19.x document parses and compiles
  identically; the IR restamps to `0.20.0` and the schema file renames to
  [`schemas/lute-ir-0.20.schema.json`](schemas/lute-ir-0.20.schema.json)
  (`$id` and title only). Engines gate on MAJOR, so nothing widens.
- `check-project` may report new errors on guards that could never hold —
  already dead at runtime; the diagnostic is new, the bug is not — and new
  `W-FACT-GUARANTEED` warnings on redundant guards.
- Pipelines passing `--deny W-UNPROVEN-RELATIONAL` must drop it.
- `capabilityVersion` is unchanged; the tree-sitter grammar is unchanged.

## [0.19.0] - 2026-09-24

**Lore entries: content the engine looks up instead of plays.**

Scenes and quests put a story on a **time axis** — what happens, in what
order, under which conditions. Much of a game's story sits in **space and
objects** instead: the torn page in the lab, the inscription on a door, the
key whose description changes after the fire, the line an NPC mutters as you
walk past. The player finds these in any order and reads them again, and the
only way to write one was a scene per note — scheduled, sequential, played
once. `0.19.0` adds a third document kind, `kind: lore`, whose `<entry>`
declarations say **what** the text is, **when** it is eligible, and **what**
reading it changes; the engine still decides **where** it lives — which item
spawns where, which panel shows the codex, when an NPC barks. Spec:
[`docs/proposals/scenario-dsl/0.19.0.md`](docs/proposals/scenario-dsl/0.19.0.md);
engine contract: [`docs/runtime/lore-entries.md`](docs/runtime/lore-entries.md).

### Added

- **Language — `kind: lore` and `<entry>`** — a lore document takes the
  quest-document frontmatter keys and a body of one or more top-level
  `<entry>` declarations and nothing else (a `# ` heading, `## ` shot,
  `<quest>`, or loose content is `E-GRAMMAR-NOT-ADMITTED`). `<entry>`
  carries `id` (required, project-unique), `target` (a dotted id such as
  `item.rusty_key`, shape-only), `category` (an ident such as `note` or
  `bark`, shape-only), `title` (localized like a quest title), `series` /
  `order`, and a `when` eligibility guard that joins the CEL-slot registry.
  Several entries may share a `target`.
- **Language — entry bodies** — content lines, `<match>` (recursively),
  `::set`, `::assert`, and `::retract` only; `<branch>`, `<hub>`,
  `<timeline>`, `<on>`, `<objective>`, and every `::` directive are
  `E-GRAMMAR-NOT-ADMITTED`. Each entry is its own lineId / voiceKey / code
  scope, as each quest is. Revealing knowledge is the ordinary `::assert`
  against the project's relations, checked by the usual arity/domain/tier
  rules, and scenes react through `holds(…)`.
- **Language — `entry.<id>.read`** — a reserved, engine-written `bool`
  (default `false`, run tier) readable from any CEL slot in any document
  kind: series gating (`when="entry.scientistLog1.read"`), a scene guard, a
  quest objective, a `<match on>` subject. Writing it is rejected, as
  writing `quest.*` is.
- **Runtime contract — first-read effects** — presenting an entry runs its
  body against live state; `::set` / `::assert` / `::retract` apply only
  while `entry.<id>.read` is `false`, after which the engine sets it.
  Re-reading shows the (possibly different) text and changes nothing else.
- **Language — document bundles** — a quest or lore document MAY declare a
  document `id:` (the scene `id:` shape) naming the file as a bundle
  (`haven.purserLedger`, `haven.mainChain`); it becomes `meta.id` and the
  document's `ProjectIndex` key. A lore document MAY declare `series:`,
  making every entry one series ordered by position in the file (1-based);
  a non-ident value is `E-META-VALUE`. Per-entry `series=` / `order=` stay
  for series spanning files.
- **Diagnostics** — `E-ENTRY-ATTR` (attribute shape, `order` without
  `series`, or a per-entry `series=` / `order=` in a document declaring
  `series:`), `E-ENTRY-ID-DUP` (per document in `check`, project-wide in
  `check-project`), `E-ENTRY-SERIES-ORDER` (a duplicate resolved
  `(series, order)`), and `W-ENTRY-REF-UNKNOWN` (`check-project`: an
  `entry.<id>.read` naming an id no document declares). `E-META-ID` now
  covers a quest or lore document's `id:`; `E-CONN-EPISODE-ID-DUP` covers
  quest and lore document ids, which share one namespace with scene ids,
  and its message says "document id". `E-UNKNOWN-KIND` admits `lore`.
- **IR** — artifact `kind: "lore"` with `LoreMeta` (`id?`, `title?`,
  `series?`, `contentLang?`, `extra?`, `plugin?`); a new `entry` command
  record (`addr`, `id`, `target?`, `category?`, `title?`, `titleLineId?`,
  the resolved `series?` / `order?`, `when?`, and `body`, the address of its
  body segment in the `OnCmd.body` convention); optional `QuestMeta.id`;
  `ProjectIndex.entries` (one row per entry, document order, omitted when
  empty), and a quest or lore document with an authored `id:` is indexed
  under it. The schema renames to
  [`schemas/lute-ir-0.19.schema.json`](schemas/lute-ir-0.19.schema.json)
  and gains `loreMeta`, `entryCmd`, `questMeta.id`, and `indexEntry`.
- **CLI — `lute trace <doc> --entry <id>`** presents one entry against
  mocked state: its lines, the `<match>` arm taken, and the effects a first
  read applies — or skips, when the mock seeds `entry.<id>.read: true`.
  Required for a lore document; `E-TRACE-ENTRY` on a non-lore document or
  an unknown id. **`lute run <artifact> --entry <id>`** does the same over
  a compiled lore artifact (exit `2` without it, or on another kind).
- **CLI — `lute lore <dir> [--json]`** — the world-narrative map: entries
  grouped by `target` and by `series` (in `order`), and for every asserted
  ground fact whether lore entries, scenes/quests, or both reveal it.
- **CLI — `lute new lore <name>`** scaffolds `lore/<name>.lute` with one
  `<entry>`.
- **Editors and tooling** — tree-sitter gains a top-level `entry`
  production with fold, highlight, and tag queries (nvim mirrors); the LSP
  covers `<entry>` in document symbols, folding, semantic highlighting, and
  attribute completion and hover. `lute tag` and `lute loc` walk entry
  bodies (each entry its own identity scope), and `lute compile --all`
  writes `ProjectIndex.entries`; `lute lint` excludes lore documents from
  scene metrics and counts their lines as translatable content.
- **Conformance — `lore-entry` and `lore-entry-reread`** — one entry
  presented on its first read (effects applied) and on a re-read (effects
  skipped); the harness passes the entry id from each fixture's
  `entry.txt`.
- **Examples — `docs/examples/haven/lore/`** — a `series:` bundle (the
  purser's ledger) whose pages reveal facts with `::assert` and gate on
  `entry.<id>.read`, a place inscription selecting text by state, and an
  NPC bark gated on `holds(knows(…))`.

### Changed

- **`lute new quest`** now writes a namespaced document `id:`
  (`quest.<ident>`), as `lute new lore` does (`lore.<ident>`).
- **`W-META-LEGACY` is scene-only** — it fires for a scene identity key
  beside an authored `id:`; on a quest or lore document those keys are
  already `E-META-UNKNOWN-KEY`, so the new document `id:` never draws it.

### Compatibility

- Every 0.18.x document is a valid 0.19.0 document: `kind: lore` was
  `E-UNKNOWN-KIND`, `entry.*` paths were undeclared, and `id:` in a quest
  document was `E-META-UNKNOWN-KEY`.
- IR: additive. Scene artifacts, and quest artifacts without an authored
  `id:`, are byte-identical apart from the version strings, and a project
  without lore writes no `entries` key. Engines gate on MAJOR, so a
  scene/quest consumer is unaffected; one without lore support rejects
  `kind: "lore"` as it rejects any unknown artifact kind.
- `capabilityVersion` is unchanged (no core vocabulary is added); the
  tree-sitter grammar regenerates.

## [0.18.0] - 2026-09-24

**Numeric thresholds get a pattern form; literal guards move onto `is=`.**

`<when is="…">` has always been the arm form the checker can reason about —
exhaustiveness, typo detection against the subject's domain, dead-arm
proofs — while `test="…"` is an opaque CEL guard. But numeric thresholds
(`$ >= 2`) had no pattern form, and the examples taught `test="$ == 'gold'"`
for plain literal comparisons, so the most common arm shape paid CEL's
quoting cost and the checker saw less than it could. `0.18.0` adds inclusive
numeric ranges to `is=`, gives `number` subjects real interval coverage, and
ships a warning plus a mechanical `lute fix` that moves literal comparisons
onto `is=`. Spec:
[`docs/proposals/scenario-dsl/0.18.0.md`](docs/proposals/scenario-dsl/0.18.0.md).

### Added

- **Language — range literals in `<when is>`** — `N..M`, `N..`, `..M`, both
  bounds inclusive, signed decimal bounds (`-3..-1`, `0.5..1.5`), freely
  mixed with other literals (`is="..0 | 10.."`). An alternative containing
  `..` is always a range, never an enum member. Lowers to the existing
  `>=` / `<=` / `&&` operators in `MatchArm.expr`.
- **Language — number-domain coverage** — a subject declared `number`
  (schema decl or component param) is covered by the union of its arms'
  intervals over the reals. `E-NONEXHAUSTIVE` names the first uncovered gap
  (`..0` + `1..` leaves `(0, 1)`); `..0` + `0..` is exhaustive without
  `<otherwise>`. `E-ARM-DEAD` catches an arm inside earlier unguarded
  coverage (`1..5` then `2..4`), `W-OTHERWISE-DEAD` a redundant
  `<otherwise>`. A partially overlapping range does not warn — `3..` then
  `1..` is the descending-threshold cascade; a covered point literal still
  warns `W-OVERLAP-ARMS`.
- **Language — `E-WHEN-RANGE`** — a malformed (`..`, `a..b`, `1...2`,
  `1..2..3`) or empty (`3..1`) range literal, anchored at the literal.
- **Language — `W-WHEN-TEST-LITERAL`** — a `<when>` without `is=` whose
  `test` is exactly `$ == L`, `$ in [L, …]`, `$ >= N`, `$ <= N`, or a
  `$ >= A && $ <= B` pair (either operand order). The warning carries a
  `migrate` fixit, so the LSP offers it as a quick fix and **`lute fix`
  applies it** (`test="$ in ['silver', 'bronze']"` →
  `is="silver|bronze"`, `test="$ >= 2"` → `is="2.."`). Only literals that
  round-trip through the `is=` classifier are rewritten — `'1'`, `'true'`,
  `'a b'` stay guards. The guard form remains valid.
- **Conformance — `match-range`** — a numeric subject selecting a range arm
  on its inclusive upper bound.
- **Editors** — the tree-sitter `when_literal` token lexes every range
  shape; LSP hover on an `is=` value over a `number` subject names the
  number domain and the inclusive-range rule.

### Changed

- **Examples and docs use `is=` for literal arms** — every living example
  and website page was migrated with `lute fix`; the frozen proposal stack
  and historical plans are untouched. Compiled snapshots of migrated
  examples show an empty debug `MatchArm.test` for rewritten arms and an
  `==`/`||` tree where `$ in [...]` used to lower to `in`; behavior is
  identical.
- **One `is=` classifier** — the checker, compiler, component folding, and
  `lute trace` now share `lute_syntax::is_pattern`, so a literal cannot mean
  one thing statically and another at runtime. Side effect: `lute trace` and
  component folding no longer match a string subject against a
  number-looking or `true`/`false` literal, matching what compiled
  artifacts always did.
- **`lute fix` output** — reports `applied N fix(es)` / `nothing to fix`
  instead of the stale `migrated … to 0.2.2`, since it now carries rules
  from several releases.

### Compatibility

- Every 0.17.x document is a valid 0.18.0 document; no existing literal
  changes meaning. New diagnostics on existing documents are the
  `W-WHEN-TEST-LITERAL` warning and, on `number` subjects only, newly
  provable dead/overlapping point arms (`1` and `1.0` are now one point).
- IR: no shape change — the version restamps to `0.18.0` per the
  alignment rule and the schema file renames to
  `schemas/lute-ir-0.18.schema.json`. Engines gate on MAJOR, so nothing
  widens.
- `capabilityVersion` is unchanged; the tree-sitter grammar regenerates.

## [0.17.2] - 2026-09-18

**Plugin owner metadata for passthrough IR records.**

### Added

- **Plugin owner identity in `kind: "plugin"` records** — passthrough plugin
  commands now carry the resolved owning package id in the optional `plugin`
  field. Hosts can dispatch and diagnose extension records by `(plugin, tag)`.
- **Typed projection fixture** — generic presentation directives prove that
  declarative directives lower to core `background`/`sprite` records while
  host-owned directives remain typed plugin records.
- **Normative specs** — plugin-system `0.0.7` defines owner metadata and
  scenario DSL `0.17.2` records the additive IR contract.

### Compatibility

- The source grammar and static semantics are unchanged.
- `plugin` is append-only on `cmdPlugin`; consumers that ignore unknown fields
  remain compatible.
- `capabilityVersion` is unchanged. Plugin ids and versions already identify
  the active owner set.

## [0.17.1] - 2026-09-21

**Neutral public history.**

Toolchain-only alignment restamp. The repository history was rewritten so
that every example, conformance fixture, snapshot, and design note uses
neutral, self-contained names: example cast and project identifiers were
renamed byte-length-preserving (every span, column, and snapshot is
unchanged), and one internal adoption assessment that was never a normative
source (`docs/adoption/`) was dropped along with references to it. Neither the
language nor the IR earns the move; both restamp per
[`docs/versioning.md`](docs/versioning.md)'s alignment rule.

### Changed

- **Examples and fixtures use neutral identifiers** — `docs/examples/`,
  `conformance/`, and the compile snapshots carry renamed cast/project ids.
  Behavior of every command is unchanged; the rename is same-length so the
  checker's byte spans and the e2e snapshots are byte-for-byte stable.
- **IR is a pure restamp** — no field added, renamed, moved, or retyped.
  `schemas/lute-ir-0.17.schema.json` keeps its name and `$id`; a `0.17`
  engine parses a `0.17.1` artifact unchanged.
- **Language is a pure restamp** — `LUTE_LANG_VERSION` advances to `0.17.1`
  so `W-LUTE-VERSION-STALE` fires on a document stamped `0.17.0` (mechanical
  fix: restamp to `0.17.1`). Spec:
  [`docs/proposals/scenario-dsl/0.17.1.md`](docs/proposals/scenario-dsl/0.17.1.md).
- `capabilityVersion` does NOT move this release.
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.17.1`.

### Removed

- `docs/adoption/` (internal adoption assessment; not a normative source).

## [0.17.0] - 2026-09-14

**Checked continuations and least-authority compilation.**

This release adds a checked, append-only continuation compiler and generic
capability-permission ceilings. Both features reuse the existing language and
ordinary artifact contract: streaming recompiles accepted source through the
whole-document pipeline, while permissions can only narrow the capabilities a
resolved document may use. The language and IR axes therefore move to `0.17.0`
as alignment restamps with no grammar, static-semantic, or IR-shape change.

### Added

- **Checked streaming continuation compiler** —
  `lute_compile::streaming::ContinuationCompiler` accepts a resolved, checked
  scene template and append-only ordinary Lute shot-body text, then emits a
  complete ordinary `Artifact` snapshot for each accepted top-level unit.
  Each unit passes through the existing checker, normalization, component
  expansion, stage injection, lowering, and addressing pipeline. Updates carry
  a monotonic `sequence` and `append_from`; prior commands and state entries
  must remain semantically unchanged, with `E-STREAM-PREFIX-CHANGED` rejecting
  retroactive changes. Normative contract:
  [`docs/proposals/scenario-dsl/0.17.0.md`](docs/proposals/scenario-dsl/0.17.0.md);
  implementation design:
  [`docs/superpowers/specs/2026-09-14-streaming-continuation-compiler-design.md`](docs/superpowers/specs/2026-09-14-streaming-continuation-compiler-design.md);
  runtime guide:
  [`docs/runtime/incremental-continuations.md`](docs/runtime/incremental-continuations.md).
- **`lute compile-stream` NDJSON transport** —
  `lute compile-stream <scene.lute> [--project DIR] [--providers DIR]` resolves
  the host-owned template once, reads body text from stdin, and flushes
  `start`, every accepted `update`, then `finish` or `error`. EOF finalizes a
  complete final leaf; exit `0` means successful finalization, `1` means
  syntax, semantic, or service rejection, and `2` means invocation, I/O, or
  invalid UTF-8. The lower-level
  [`lute_syntax::incremental::IncrementalContinuationParser`](crates/lute-syntax/src/incremental.rs)
  remains available for lossless syntax framing without checking or artifact
  production.
- **Generic capability permissions** — trusted `lute.project.yaml` root and
  profile policy can restrict directives, scalar-state writes, fact writes,
  bridge `service/operation` pairs, declarative rewards, and quests. Missing
  fields are unrestricted; explicit empty lists deny the category. Project,
  `global`, ancestor, selected-profile, and host layers compose
  conjunctively. `--permission-profile NAME` adds an independently trusted
  ceiling to `check`, `compile` (including all-or-nothing `--all`),
  `compile-stream`, and `context` without activating plugins or rewriting the
  source-selected profile. Non-suppressible `E-PERMISSION-*` diagnostics
  cover defaults, seed facts, plugin effects and bridges, nested/transitive
  components, quests, and rewards; the compiler rechecks policy before
  lowering. Normative contract:
  [`docs/proposals/plugin-system/0.0.6.md`](docs/proposals/plugin-system/0.0.6.md);
  security and host guide:
  [`docs/runtime/capability-permissions.md`](docs/runtime/capability-permissions.md);
  implementation design:
  [`docs/superpowers/specs/2026-09-14-capability-permissions-design.md`](docs/superpowers/specs/2026-09-14-capability-permissions-design.md);
  runnable example:
  [`docs/examples/capability-permissions/`](docs/examples/capability-permissions/).

### Changed

- **Release-axis alignment** — the toolchain, language, and IR versions all
  advance to `0.17.0` under
  [`docs/versioning.md`](docs/versioning.md). Language `0.17.0` is
  byte-for-byte `0.16.0` grammar and static semantics; IR `0.17.0` has no
  field, command, or serialization-shape change.
- **IR schema restamped per release line** —
  `schemas/lute-ir-0.16.schema.json` is renamed to
  [`schemas/lute-ir-0.17.schema.json`](schemas/lute-ir-0.17.schema.json), with
  `$id` and title updated and the schema body otherwise unchanged. The runtime
  gate remains MAJOR-only, so consuming engines require no schema migration.
- **Capability snapshot hashing includes restrictive policy** — an effective
  permission ceiling participates in `capabilityVersion`, while unrestricted
  projects retain their existing capability hash byte-for-byte.

## [0.16.0] - 2026-09-01

**Rewards become data.**

Conditions have been first-class, statically checked surface since the
scene kind shipped; rewards were only operational — plugin `::grant`
directives buried in `<on questComplete>` bodies. Execution was correct
(exactly-once, quest-scoped since 0.14.0), but the artifact carried
rewards as commands inside a handler, so nothing read "this quest's
rewards" as data: no journal preview at accept time, no balancing
extraction, no reward-shaped lint, and a conditional reward equally
invisible. `0.16.0` closes the half that was missing. `<reward/>` — a
self-closing element, direct child of `<quest>` or `<objective>`, with
`kind` / `target` / `amount` (integer scalar or the new `N..M` range
literal, negatives legal) / `when` (ordinary CEL slot) / quest-only `on`
(`complete` default, or `failed`) — lowers to pure data on the owning
records (`QuestCmd.rewards` / `ObjectiveEntry.rewards`), and the
reference runner and `lute trace` emit deterministic `grant` transcript
events at each fresh transition (spec §3 D-D). The engine still grants;
the language never rolls a range or synthesizes a `::grant`.

### Added

- **Language — declarative `<reward/>` element on `<quest>` /
  `<objective>`** — a self-closing owner field, direct child of the
  enclosing quest or objective; anywhere else, a `<reward>` with a body,
  or an unknown attribute in its closed set is rejected through the same
  per-tag closure that catches every other misplaced construct
  (`E-UNKNOWN-ATTR`, 0.10.0 §D-J). Attributes: `kind` (required
  non-empty string, the vocabulary key), `target` (optional string, the
  rewarded id — item / currency / quest / …), `amount` (optional integer
  **or** range literal `N..M` with integer bounds and `N <= M`; default
  `1`; negatives are real deductions), `when` (optional `CelString`,
  evaluated at the grant instant; joins the CEL-slot registry with
  `E-CEL-PROFILE` / `E-MAYBE-UNSET` / unset-sentinel guards / LSP hover /
  fill), and quest-level `on` (`complete` default or `failed` — which
  terminal transition grants it; on an objective-level entry `on` is
  rejected). Range amounts are declarations, not rolls: the journal
  shows "1–5", a balancer computes expectation, and the reference
  runtime never rolls — dice belong to the engine (0.0.1). Spec:
  [`docs/proposals/scenario-dsl/0.16.0.md`](docs/proposals/scenario-dsl/0.16.0.md).
  Design record: [`docs/superpowers/specs/2026-09-01-lute-reward-design.md`](docs/superpowers/specs/2026-09-01-lute-reward-design.md).
- **Language — `E-REWARD-ATTR`** — shape violation, anchored at the
  offending attribute: empty `kind`, malformed `amount` (non-integer,
  bad range, `N > M`), `on=` on an objective-level reward, or an `on`
  value outside the closed `complete` / `failed` enum.
- **Language — `E-REWARD-KIND`** — vocabulary violation when the
  resolved capability set declares any `rewardKinds:`: an unknown
  `kind`, a `target` present/absent against the kind's contract, or a
  `target` failing its provider domain. Stale snapshots degrade to the
  usual *catalog-stale* grade, never a hard error. With no
  `rewardKinds:` declared, shape checks stand alone and any kind name
  admits.
- **Manifest — `rewardKinds:` plugin export** — a map of kind id →
  declaration: optional `target` contract naming a provider domain (the
  same `providerRef` pattern directive attrs use — resolved against
  pinned snapshots, Fresh / *catalog-stale* / unknown-id) plus optional
  extra attr schema for game-specific slots. Folded into the capability
  snapshot as a guarded, sorted section: a populated vocabulary moves
  `capabilityVersion`, while the empty core section hashes
  byte-identically — no restamp for projects without a
  `rewardKinds:`-declaring plugin.
- **IR — `QuestCmd.rewards` and `ObjectiveEntry.rewards`** — new arrays
  of `RewardEntry`, `skip_serializing_if = "Vec::is_empty"`, so a
  rewardless artifact stays byte-identical to `0.15.1` output.
  `RewardEntry` serializes `kind`, optional `target`, exactly one of
  `amount` XOR (`amountMin` + `amountMax`) after amount defaulting
  (unauthored → `amount: 1`), optional `when` (`CelPair`), and `on:
  "failed"` only on a quest-level entry (`on: "complete"` is the
  omitted default; never emitted on objective entries). Range amounts
  are carried verbatim on the wire (spec D-C: never pre-rolled).
- **Runtime — deterministic `grant` transcript events** — `lute run` /
  `play` and `lute trace` emit a structured grant event at each fresh
  transition (spec §3 D-D): at `→ complete` the quest's `on="complete"`
  rewards, at `→ failed` (including cascade child-failure, 0.14.0) its
  `on="failed"` rewards, and when an objective first becomes `done`
  that objective's rewards. Each reward grants **at most once per quest
  instance** (same monotonicity as objective bodies); `when` is
  evaluated at the grant instant against the transition's own
  state/fact snapshot, a non-`true` verdict skips the reward once (not
  re-armed); order within one owner is declaration order, and when one
  step settles both an objective and its quest, objective grants
  precede quest grants — all grants precede the corresponding lifecycle
  handler body, so handler narrative can react to what was just
  granted. Range amounts pass through as
  `amountMin`/`amountMax` unchanged, keeping reference output
  byte-deterministic; the engine resolves `N..M` however it likes.
- **Grammar — tree-sitter `reward` production** — the `<reward/>`
  element joins the closed tag set (tags are enumerated), so
  editor grammars see it after regeneration (`tree-sitter generate`).

### Changed

- **Schema renamed per release line** —
  `schemas/lute-ir-0.15.schema.json` is renamed to
  `schemas/lute-ir-0.16.schema.json`;
  its content gains the `rewardEntry` definition (`kind` required,
  optional `target`, exactly one of `amount` / (`amountMin` +
  `amountMax`), optional `when`, quest-only `on: "failed"`) and the two
  `rewards` arrays on `questCmd` and `objectiveEntry`. `$id` and title
  updated to match. Under the `0.13.0` MAJOR-only gate the rename
  tracks the published release line for strict validators only; no
  engine gate widens, so a `0.15` engine parses a `0.16.0` artifact
  unchanged and simply does not ask for the added arrays.
- **`docs/runtime/quest-lifecycle.md` gains a Rewards section** — the
  when / `when=` / order / range rules the engine now grants against
  (§3 D-D), added to the normative runtime contract so a consuming
  engine has one place to read the semantics from.
- **Example corpus gains reward coverage** — quest fixtures under
  `docs/examples/` grow `<reward/>` declarations exercising the range
  literal, conditional `when=`, `on="failed"`, and objective-level
  entries; scene fixtures are untouched.
- `capabilityVersion` does NOT move this release for any project
  without a `rewardKinds:`-declaring plugin: the new snapshot section
  is guarded and its empty core value hashes byte-identically. The
  tree-sitter grammar gains one production — a grammar regeneration,
  not a capability restamp (the stamp tracks the core snapshot).
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.16.0`.

## [0.15.1] - 2026-09-01

**LSP joins the npm distribution.**

Toolchain-only alignment restamp. The npm launcher `@lute-lang/lute`
now ships `lute-lsp` alongside `lute`: a second `bin` entry
(`packages/cli/lsp-bin.js`) resolves the platform-specific core package
and dispatches to its `lute-lsp` executable, so downstream editors can
spawn the language server through the same install the CLI already uses
(a `bunx @lute-lang/lute-lsp` route, symmetric with `bunx @lute-lang/lute`).
Every per-platform core package (`@lute-lang/lute-core-darwin-arm64`,
`@lute-lang/lute-core-linux-x64`, `@lute-lang/lute-core-win32-x64`) now
carries both binaries, and the publish/build-native workflow matrices
build `-p lute-lsp` next to `-p lute-cli`. Neither the language nor the
IR earns the move; both restamp per
[`docs/versioning.md`](docs/versioning.md)'s alignment rule.

### Changed

- **npm distribution ships `lute-lsp` alongside `lute`** — the
  `@lute-lang/lute` launcher gains a second `bin` entry, `lute-lsp`,
  backed by `packages/cli/lsp-bin.js` (the CLI launcher's
  platform-resolution logic reused, dispatching to the same
  `@lute-lang/lute-core-<platform>` package). The per-platform core
  packages now carry both `lute` and `lute-lsp` binaries; `publish.yml`
  and `build-native.yml` build both under one release matrix. No
  behavior change to either binary — this is packaging only.
- **IR is a pure restamp** — no field added, renamed, moved, or
  retyped. `schemas/lute-ir-0.15.schema.json` keeps its name and `$id`
  (the file's `0.15.x` title already covers `0.15.1`, and the schema is
  byte-identical to `0.15.0`); no engine gate widens under `0.13.0`'s
  MAJOR-only runtime contract, so a `0.15` engine parses a `0.15.1`
  artifact unchanged.
- **Language is a pure restamp** — no grammar production, no
  static-semantic rule, no diagnostic added or removed;
  `LUTE_LANG_VERSION` advances to `0.15.1` so `W-LUTE-VERSION-STALE`
  fires on a document stamped `0.15.0` (mechanical fix: restamp to
  `0.15.1`; a `0.15.0`-clean document restamped to `0.15.1` checks
  clean with no other edit). Spec:
  [`docs/proposals/scenario-dsl/0.15.1.md`](docs/proposals/scenario-dsl/0.15.1.md).
- `capabilityVersion` does NOT move this release: no `lute.core`
  export changes, no grammar production change.
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.15.1`.

## [0.15.0] - 2026-09-01

**Scenes get to name themselves.**

The canonical scene key was the derived join `{character}.{episodeId}`,
and three required frontmatter keys had degenerated into an opaque id
authored in three pieces, two forced to be integers. `0.15.0` adds one
opt-in scene frontmatter key — `id:` — that IS the canonical scene key
wherever the derived join was consumed today, demotes the four legacy
identity keys to optional when it is present, and lands a free descriptive
`extra:` block on scene and quest roots for anything the language never
read anyway. No grammar production changes (frontmatter is one opaque
token), `capabilityVersion` does not move, and untouched corpora keep
every lineId, translation, `visited()` reference, and `schedule.yaml`
behavior — their artifacts differ from the 0.14.0 output only by the
added `meta.id` line.

### Added

- **Language — authored canonical scene id via `id:`** — one scene-only
  frontmatter key that IS the canonical scene key everywhere the derived
  `{character}.{episodeId}` join is consumed today: the lineId prefix and
  the structural choice/hub option ids, `visited()` targets (including the
  nearest-key suggestion on `E-CONN-UNKNOWN-NODE`), connectivity nodes,
  reachability, project-wide duplicate detection, `prereqEdges[].node`,
  the document key in `project.index.json`, and the runtime visited-set
  entry `lute play` records after presentation. Value is non-empty and
  matches `[A-Za-z0-9_.-]+` (`.` admits namespacing like
  `haven.s01ep01`); anything else is **`E-META-ID`**. When `id:` is
  present the four legacy identity keys (`character` / `season` /
  `episode` / `episodeId`) are no longer required and **`E-META-MISSING`**
  fires only when the defaults-merged frontmatter lacks `id:`. Authored
  and derived keys share one namespace and one project-wide uniqueness
  rule: **`E-CONN-EPISODE-ID-DUP`** keeps its code (tooling stability)
  but its message now names the *canonical scene id*, and an occurrence
  contributed by an authored `id:` anchors at that key. `id` is not
  defaultable (a manifest supplying one to many documents can only
  manufacture collisions), so it stays outside `defaults:`' closed key
  set. When absent, every site derives `{character}.{episodeId}` exactly
  as 0.14.0 — the fallback is the existing code path, so an untouched
  project's lineIds, translations, `visited()` references, and
  `schedule.yaml` behavior do not move. Spec:
  [`docs/proposals/scenario-dsl/0.15.0.md`](docs/proposals/scenario-dsl/0.15.0.md).
  Design record: [`docs/superpowers/specs/2026-09-01-lute-scene-id-design.md`](docs/superpowers/specs/2026-09-01-lute-scene-id-design.md).
- **Language — free descriptive `extra:` block on scene and quest roots**
  — one reserved frontmatter key holding an open mapping (keys free,
  values scalars or flat scalar-lists; a nested mapping or a non-scalar
  list entry is **`E-META-VALUE`**), legal on scene and quest roots and
  rejected on schema/component documents through the existing kind gate.
  `extra:` carries **no language semantics**: not readable from CEL, not
  a state path, not a template token, consulted by no checker, compiler,
  or runtime rule. It is serialized verbatim into `meta.extra` (omitted
  when empty) so external tooling — search, TMS, editors — can key on
  it, and the top level stays closed (`E-META-UNKNOWN-KEY` keeps
  catching top-level typos). `extra` joins `defaults:`' closed key set;
  0.10.0 §6.2's whole-value-per-key rule applies unchanged.
- **Language — `W-META-LEGACY`** — a document that authors `id:` and
  ALSO authors any of `character:` / `season:` / `episode:` /
  `episodeId:` in its **own** frontmatter draws one warning per key,
  anchored at that key: identity now comes from `id:`; move the value
  under `extra:` if it should stay searchable. Promotable with `--deny`.
  The pass reads the authored frontmatter, never the defaults-merged
  view: a manifest-inherited `character:` on an `id:`-carrying document
  is silent. A document with no `id:` warns nowhere; the derived path is
  fully supported and its removal is deferred to a future major.
- **IR — `SceneMeta.id` (required) and `extra` on both `SceneMeta` and
  `QuestMeta` (optional)** — `id` is always emitted as the resolved
  canonical scene key (authored `id:` verbatim, else the derived
  `{character}.{episodeId}` join), so no consumer rederives identity;
  the four legacy `SceneMeta` fields (`character`/`season`/`episode`/
  `episodeId`) demote to optional and are emitted only when the source
  supplies them; both metas gain a `BTreeMap`-backed `extra` block
  (key-sorted, omitted when empty), populated from the frontmatter
  block. Additive under `0.13.0`'s MAJOR-only gate — the only artifact
  delta on a pre-0.15 document is the added `meta.id` line, and a
  `0.14` engine parses a `0.15.0` artifact unchanged.

### Changed

- **Schema renamed per release line** — `schemas/lute-ir-0.14.schema.json`
  is renamed to [`schemas/lute-ir-0.15.schema.json`](schemas/lute-ir-0.15.schema.json);
  its `sceneMeta` now requires `id`, admits the four legacy identity
  keys as optional, and gains an `extra` object (values are scalars or
  flat scalar-lists); `questMeta` also gains `extra`. `$id` and title
  updated to match. Under the MAJOR-only gate the rename tracks the
  published release line for strict validators only; no engine gate
  widens.
- **`E-META-MISSING` narrowed and `E-CONN-EPISODE-ID-DUP` generalized**
  — the former fires only when the defaults-merged frontmatter lacks
  `id:` (so an `id:`-carrying document with no `character:`/`season:`/
  `episode:` is silent); the latter keeps its code but its message
  names the *canonical scene id*, and an occurrence contributed by an
  authored `id:` anchors at `id:` instead of `character:`.
- **Clippy 1.96 lint-debt cleanup** — doc-list indentation, identical-if
  merges, `if let` chains, and scoped `#[allow(…)]`s for
  `result_large_err` / `too_many_arguments`. No behavior change; the
  workspace is clippy-clean again on the current stable toolchain.
- `capabilityVersion` does NOT move this release: no `lute.core` export
  changes, and frontmatter admission is checker work, not a capability
  export.
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.15.0`.

## [0.14.0] - 2026-08-31

**Quests that name their sub-quests.**

A parent objective can now name a child quest by id — `<objective id
quest="childId"/>` — and the child's completion IS that objective. The
language finally owns the vocabulary for the parent–child structure games
model constantly (BG3 "Save the Grove" → "Free Halsin"), so the checker
can catch orphaned children, the compiler can synthesize both the parent's
per-child completion tests and its per-required-child failure disjunction,
and engines can reconstruct a project-wide journal tree by unioning one
new IR field. `abandoned` is explicitly rejected as a fifth lifecycle
state (a nuance of journal copy is not worth a state-enum ripple).

### Added

- **Language — subquest support via `<objective quest=…>`** — an
  objective can now name a child quest by id (`<objective id
  quest="childId"/>`), and the child's completion is the objective.
  `quest=` and `done=` are mutually exclusive on one objective
  (`E-OBJECTIVE-QUEST-DONE`, exactly one required); every other objective
  attribute admits alongside `quest=` (`when=`, `optional`, `title=`, a
  completion body), and the body still plays exactly once — when the
  objective first becomes `done`, i.e. when the child completes — the
  natural "child resolved" journal slot. The compiler rewrites the
  objective's `done` as `quest.<child>.state == 'complete'` and the
  parent's `fail` as the disjunction of the authored predicate (if any)
  and one `quest.<c>.state == 'failed'` test per **required** child in
  document order, so the parent's derived-completion machinery and
  `fail`-before-completion precedence are unchanged and an engine
  unaware of the feature evaluates the compiled artifact correctly with
  zero code changes. The two rules a per-artifact compile cannot
  synthesize (a child does not know its parent) are documented in
  [`docs/runtime/quest-lifecycle.md`](docs/runtime/quest-lifecycle.md):
  (1) a terminal transition on a parent cascades every still-`active`
  child to `failed` (recursive; a required child cannot be `active` at
  parent `complete`, so that arm only ever fails running optionals) and
  (2) a referenced child with no `start` activates when its parent
  activates (replacing the walk-start / accept-driven default; a `start`
  predicate on a referenced child is evaluated only while the parent is
  `active`). Project shape is a **tree, not a DAG** — at most one parent
  per quest, cycles rejected, depth unbounded — guarded by three new
  project-level diagnostics `E-QUEST-REF-UNKNOWN` (same-document
  resolution at `check`, cross-document at `check-project`, matching how
  `after` targets already split), `E-QUEST-MULTI-PARENT`, and
  `E-QUEST-TREE-CYCLE` (self-reference is a length-1 cycle and, when
  parent and child share a document, `check` catches it early), plus the
  reachability extension where an `E-QUEST-UNREACHABLE` child propagates
  `E-OBJECTIVE-UNSATISFIABLE` onto its referencing objective's `quest=`
  span. Design record:
  [`docs/superpowers/specs/2026-08-31-lute-subquest-design.md`](docs/superpowers/specs/2026-08-31-lute-subquest-design.md).
  Language reference: [Quests & scenes → Subquests](https://lute-lang.vercel.app/language/quests-and-scenes/#subquests).
  Worked example: [`docs/examples/quest-subquest.lute`](docs/examples/quest-subquest.lute).
- **IR — `ObjectiveEntry.quest`** — the new field carries the referenced
  child id when authored; it is serialized only for subquest objectives
  (`skip_serializing_if = "Option::is_none"`, appended after `body`), so
  artifacts from documents without the feature are byte-identical.
  Engines reconstruct the parent→child tree by unioning the field across
  artifacts, exactly as they already union `relations`, `rules`, and
  `prereqEdges` — no new command kind, no new edge table.

### Fixed

- **`lute run` fired every quest's lifecycle handlers on every quest's
  transition.** The reference runner matched `<on>` handlers by event name
  alone, so in a multi-quest artifact one quest's `questComplete` ran EVERY
  quest's `questComplete` bodies — three completions replayed the same
  narrator line three times. Handlers now carry their enclosing quest
  (recovered from stream order — an `on` record follows its own quest
  declaration head) and the engine-derived lifecycle events
  (`questActive`/`questComplete`/`questFailed`) fire only for their own
  quest, which is what quest-lifecycle.md always said; mock `events:` (world
  events) stay unscoped. Surfaced by the subquest work — a cascading child's
  `questFailed` under the old matching would have replayed every sibling's
  failure copy — but the bug predates it and needed no subquests to trigger.
  The runner also implements the two subquest engine rules now, exactly as
  [`docs/runtime/quest-lifecycle.md`](docs/runtime/quest-lifecycle.md)
  specifies them for any engine: a referenced child activates on its
  parent's activation (its `start`, if any, is evaluated only while the
  parent is `active`; it is not accept-driven and never activates at walk
  start) and a terminal parent cascades every still-`active` child to
  `failed`, recursively, firing each child's own `questFailed`.

## [0.13.0] - 2026-08-26

**Editorial policy as configuration, and drafts that stay legible.**

Two axes earn this one. The **toolchain** gains a lint layer: `lute lint`
evaluates configurable editorial content rules — the checks a scenario team
enforces by convention (dialogue length and ratio, scene-length spread,
per-speaker emotion distribution with streak caps and a thrash floor,
variant composition, asset existence, shot staging) — as project policy in
`lute.lint.yaml` rather than as hardcoded opinion. The **language** gains
one universal frontmatter key, `codesLocked:`, which marks a document's
line codes as published identity and lets the new `lute tag --force`
renumber freely everywhere it is absent. The release also relaxes the
runtime version gate: engines now refuse on a **major** mismatch only, so
the recurring "pure restamp, but every engine must widen its gate" cost
(paid on 0.11.0 and 0.12.0 back to back) is gone.

### Added

- **Lint system** — `lute lint` evaluates configurable editorial content
  rules independently of `lute check`, with human and JSON diagnostics,
  deniable `L-*` rule codes, and LSP opt-in (`lsp: true`). `lute.lint.yaml`
  configures levels (`off`/`hint`/`info`/`warn`/`error`), thresholds,
  ignore globs, and project-local CEL rules over core-computed metric
  tables (line/shot/scene/speaker/group/project); seven core rules ship
  enabled with drafting-safe defaults. Plugins may export advisory
  `lints/*.yaml` rules (`<plugin-id>/<rule-id>`,
  [`plugin-system/0.0.5.md`](docs/proposals/plugin-system/0.0.5.md))
  **without** changing the capability snapshot or `capabilityVersion` —
  lints are advisory and never move artifact identity. Guide:
  [`docs/linting.md`](docs/linting.md).
- **Language 0.13.0 — `codesLocked:` and the guarded renumber** —
  `lute tag --force` rewrites every content line's `code` in clean document
  order (0010/0020/… per speaker per identity scope), a drafting tool for
  sequences left gappy by edits; output is indistinguishable from a fresh
  tag pass and the run is idempotent. The new universal frontmatter key
  `codesLocked:` refuses it — published codes key `lineId`/`voiceKey`, and
  renumbering them severs the localization/voice join. The guard fails
  closed (any value other than exactly `false` locks), and plain `lute tag`
  back-fill stays available under lock. Spec:
  [`docs/proposals/scenario-dsl/0.13.0.md`](docs/proposals/scenario-dsl/0.13.0.md).

### Changed

- **Version negotiation gates on MAJOR only** — the runtime contract
  ([`docs/runtime/execution-model.md`](docs/runtime/execution-model.md))
  and the reference runner (`lute run`/`play`/`test`) now refuse an
  artifact only when its `irVersion` MAJOR differs from the implemented
  line; minor and patch are compatible-by-default (append-only fields,
  ignored when unknown), and an **unknown command `kind`** remains the
  hard error that catches a genuinely newer capability. Previously a
  minor-line mismatch was refused outright, which taxed every aligned
  release — `0.11.0` and `0.12.0` both moved the gated line while
  changing nothing an engine reads. Pre-1.0 caveat: breaking IR changes
  may still land in a minor (`0.10.0`'s provenance rename); they are
  called out in this changelog and the schema, no longer fenced by the
  gate.
- **IR 0.13.0 is a pure restamp** — no field is added, renamed, moved, or
  retyped. The schema file still tracks the release line for strict
  validators, so `schemas/lute-ir-0.12.schema.json` is renamed to
  [`schemas/lute-ir-0.13.schema.json`](schemas/lute-ir-0.13.schema.json)
  (body unchanged apart from the stamp) — but under the major-only gate
  this rename no longer implies any engine edit at all.
- `capabilityVersion` does NOT move this release: plugin `lints` exports
  are excluded from the capability snapshot by design, and `lute.core`
  declares nothing new (`codesLocked` is checker frontmatter admission,
  not a capability export).
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.13.0`.

## [0.12.0] - 2026-08-19

**Flow that names its destinations.**

The release-earning axis is the **language**: forward jumps. A document can
now label a position — `::mark{id="x"}` anywhere, or `id="x"` directly on a
content line — and move to it with `::next{to="x"}`, optionally guarded
(`::next{to="x" when="<CEL>"}`: jump when true, fall through when false).
Jumps are FORWARD-ONLY by static rule, so the walk stays a DAG and every
existing analysis (reachability, definite assignment, trace termination,
coverage) keeps its footing. Combined with `::end{reason=…}`, branches can
now leave their arm, rejoin a later trunk, re-diverge, and land on multiple
endings without nesting.

### Added

- **Language 0.12.0 — labels and forward jumps** — `::mark{id=…}` (position
  anchor, emits no record), content-line `id=…` (that line's record is the
  label; the one line attribute that is compile-time addressing rather than
  a record field), `::next{to=… when=…}`. One label namespace per document.
  New diagnostics: `E-MARK-DUP` (duplicate label, mark/line-id cross
  collisions included), `E-NEXT-UNDEFINED`, `E-NEXT-BACKWARD` (forward-only),
  `W-CODE-AFTER-NEXT` (dead nodes after an unguarded `::next`, the
  `W-CODE-AFTER-END` mirror). Guarded `::next` desugars to the same canonical
  one-arm `<match>` a guarded content line lowers to. Spec:
  [`docs/proposals/scenario-dsl/0.12.0.md`](docs/proposals/scenario-dsl/0.12.0.md).
- Timeline clips explicitly reject `::mark`/`::next` (the `::end` precedent).

### Changed

- **IR 0.12.0 is a pure restamp** — `::next` lowers to the EXISTING `jump`
  command and guarded jumps to the existing `match` record; no field is
  added, renamed, moved, or retyped. The gated `major.minor` still moves
  (`0.11` → `0.12`) purely by the alignment rule, so
  `schemas/lute-ir-0.11.schema.json` is renamed to
  [`schemas/lute-ir-0.12.schema.json`](schemas/lute-ir-0.12.schema.json)
  (body unchanged apart from the stamp) and an engine gated on IR `0.11`
  must widen its gate to `0.12` — reading no new field once it does.
- `lute.core` capability surface grows two directives (`mark`, `next`), so
  `capabilityVersion` snapshots move.
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.12.0`.

## [0.11.1] - 2026-08-19

**A branch that asks its question out loud, and starts a clock.**

The release-earning axis is the **language**: `<branch>` gains two optional
attributes. `prompt="…"` names what the choice is ABOUT — the situation
sentence a host UI shows above the option labels — and `timeout="N"` gives the
pick a positive-integer seconds budget, for hosts that run a countdown and
emit a timeout when the reader does not choose. Both are author-optional;
every existing document is untouched.

### Added

- **Language 0.11.1 — `<branch prompt=… timeout=…>`** — two new optional
  `<branch>` attributes. The checker admits them (`E-UNKNOWN-ATTR` no longer
  fires) and validates their values: `prompt` must be a non-empty string
  (`E-BRANCH-PROMPT`), `timeout` must parse as a positive integer
  (`E-BRANCH-TIMEOUT`; `"0"` and non-numeric values are rejected at the
  attribute's own span). `<hub>` is unchanged.
- **IR 0.11.1 — `prompt` / `timeoutSec` on the choice record** — the compiled
  choice command carries the two values when authored and omits both fields
  entirely when not, so artifacts from prompt-less documents are byte-stable.
  Additive-only: the schema file stays
  [`schemas/lute-ir-0.11.schema.json`](schemas/lute-ir-0.11.schema.json) (the
  gated `major.minor` does not move) and engines already on IR `0.11` parse
  `0.11.1` artifacts unchanged.
- **`lute play` shows the ask** — a prompted branch renders as
  `▷ choice <id> "<prompt>" (<N>s): …` in playthrough transcripts.

### Changed

- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.11.1`. The toolchain and IR moves
  are consumer no-ops beyond the two optional fields above.

## [0.11.0] - 2026-08-15

**A route through the whole project, played in the order the player sees it.**
Everything below is toolchain: a new scheduling layer that places scenes on a
tick clock instead of leaving order to file position, a new command that
chains them into one reviewable transcript, and two bug fixes in the shared
reference runner that predate this release and affect `lute run` as much as
the new command. Language and IR are both content no-ops this time — see
*Changed* for the one real cost that still falls out of the alignment rule.

### Added

- **`schedule.yaml`** — a headerless, CLI-owned project file beside
  `lute.project.yaml` that places a project's scenes on a tick clock instead
  of leaving reading order to file position. A `clock:` (named buckets ×
  ticks-per-bucket × days) carries `lanes:` (`user`, single-threaded and
  guarded against overlap by default; `world`, overlap-by-design for events
  that do not wait for the player) and `placements:`, each an `event`
  occupying a `[at, at+size)` interval with one satisfiable-per-route
  `variant` (`when:` reads the same content-line CEL surface a guard already
  does) selected at play time — plus `optional:` (legal to have no
  satisfiable variant on some route), `presentation:` (execution order is
  `(presentation, resolved at, declaration index)`, decoupling *when a scene
  is presented* from *when it happens on the story clock* — a cold-open
  flashback can present first and be story-chronological last), and a
  variant-level `at:`/`size:`/`presentation:` override so the same event can
  sit at a different position per route. Static checks cover clock structure,
  malformed/dynamic `at:`, duplicate/unknown lanes and events, missing or
  escaping `doc:` paths, unsatisfiable and ambiguous route-space assignments
  (an `assume:` list lets a schedule assert an upstream contract like
  "inflow is never `none`" so a sentinel route stops producing false gaps),
  overlapping same-lane intervals, and an idle-pacing threshold — see
  [`docs/schedule-and-play.md`](docs/schedule-and-play.md) for the full key
  and diagnostic reference. Deliberately out of language scope: no `kind:`,
  no `luteVersion:`, no capability fold, no language/IR version bump — a
  future design integrates it as a real doc kind.
- **`lute play <PROJECT_DIR>`** — plays one scheduled route through a WHOLE
  project as one chained, reviewer-facing transcript: the whole gated project
  compiles once (the same declaration union `compile --all` writes,
  including quest docs, which are never placed), then walks the schedule's
  user-lane placements in presentation order, re-evaluating each event's
  guarded variants against LIVE state and threading `run.*`/`user.*`/
  `app.*`/`quest.*` state and facts across scene boundaries through `lute
  run`'s own reference evaluator (`scene.*` always resets to the entering
  scene's own declared defaults). A scene's `after:` prerequisite is
  re-checked against the visited/completed sets accumulated in presentation
  order, not file order — a cold-open scene declared `presentation: 0` can
  legitimately run before a day-one scene it is chronologically behind.
  World-lane events interleave: after each user placement, every not-yet-fired
  world placement whose start tick falls inside the segment just covered
  drains atomically, in `(at, declaration index)` order, even under
  `--lanes user` (world scenes still execute — state must not depend on
  rendering — the flag only gates the transcript). A presentation jump
  backward starts a new segment and is purely cinematic (no state rolls
  back); a world event draining inside one is flagged
  `W-SCHED-WORLD-IN-FLASHBACK`. Route selection is `--state`/`--fact` seeds,
  a `--script <route>.play.yaml` (this module's own closed grammar — `state:`/
  `facts:`/`choose:` with EVENT-QUALIFIED choice/hub ids, `kuhen-meeting/ask:
  [ask-record]` — never the trace mock parser, whose top-level key set has no
  notion of that shape), and/or ad-hoc `--choose <event>/<id>=<choiceId>`;
  `--auto first` resolves anything left unscripted, at every hub
  re-presentation, not just the first. Any guard or effect the reference
  runner genuinely cannot resolve (`now()`/`validAt`, an unresolved plugin
  `bridgeResult`) halts the walk **incomplete** naming the surface, never a
  silent unknown. Exit `0` complete, `1` a schedule/causality violation named
  by its `E-SCHED-*` code, `2` a usage/I/O failure (including the hard error
  when a project has no `schedule.yaml` at all — there is no `after:`-graph
  fallback, since sibling route files are unguarded by design), `3`
  incomplete. `--lanes user|all`, `--steps N` (partial-playback preview),
  and `--json` (a deterministic, byte-identical-for-the-same-seeds structured
  transcript) round out the surface.
- **`lute play --coverage <FILE>…`** — the review-gap detector: replays every
  named route script through the same chain executor with per-script
  transcript rendering suppressed, then reports every placement, variant, and
  hub/choice option the corpus as a whole never exercised. Exit `0` full
  coverage, `1` a gap remains, `2` a usage/I/O failure, `3` at least one
  corpus script halted before completion. Exclusive with `--script`/
  `--choose`/`--steps` — a single playthrough's own knobs do not compose with
  a corpus replay.
- **The full `E-SCHED-*`/`W-SCHED-*` diagnostic set** — fifteen static errors
  (clock structure, duplicate buckets, unknown lanes, duplicate events,
  malformed variant form, invalid size, unparseable/dynamic `at:`, clock
  overflow, a missing or path-escaping `doc:`, an unsatisfiable or ambiguous
  route assignment, an overlapping same-lane interval, and a malformed
  guard), one runtime error (an `after:` prerequisite unsatisfied in
  presentation order), and five warnings (an unplaced scene doc, an idle-gap
  above the pacing threshold, a route-space enumeration too large to sweep, a
  scene's first `::bg time=` disagreeing with its placement's bucket, and a
  world event draining inside a rewound segment).

### Fixed

- **A compiled `<when is="…">` match arm always fell through to
  `<otherwise>`, no matter which value it named.** An `is`-form arm compiles
  to an EMPTY raw `test` string plus a structured `expr` node (IR A13) — the
  executable surface an engine is meant to read — and the reference runner's
  `do_match` evaluated only `test`, so every `is` arm read as unknown and the
  match always converged on its `otherwise` branch, regardless of the actual
  state. First observed as six onboarding routes all greeting the player with
  the fallback line. `do_match` now prefers the compiled `expr` whenever one
  is present, falling back to the raw `test` only for a `test=`-form arm.
  This shipped in `lute run` (and therefore `lute trace`'s replay of a `run`)
  since `<when is=>` existed; a project relying on a `<match>`/`<when is=>`
  for its reference transcript should re-run it against this release.
- **A hub whose scripted decision sequence ran out with an eligible,
  non-`exit` option still on the table silently left the hub instead of
  halting.** `Runner::do_hub` iterated its forced-choice vector to the end
  and fell through to whatever came after, regardless of whether every
  option had actually converged — so a mock's `choose:` list one entry short
  of a full hub visit reported a clean, complete run (exit `0`) instead of
  the incomplete walk it actually was. `do_hub` now halts incomplete, naming
  the hub and its still-eligible options, exactly like an unscripted branch
  choice already did. Affects any `lute run --mock`/`lute play` walk through
  a hub with a `once`, non-`exit` option a script does not explicitly retire.

### Changed

- **All three axes read `0.11.0`, and only the toolchain earns it.** Language
  `0.11.0` is byte-for-byte `0.10.2` (== `0.10.1` == `0.10.0`) semantics
  ([`scenario-dsl/0.11.0.md`](docs/proposals/scenario-dsl/0.11.0.md)), and the
  IR carries no shape *or* content change — genuinely nothing for a consuming
  engine to read differently. What still moves is the number: `LUTE_IR_VERSION`
  reads `0.11.0` because a release re-aligns every visible axis whether or not
  its contract changed, and that number's `major.minor` component is the one
  the runtime contract gates on. `0.10.1` and `0.10.2` both stayed inside
  `0.10`, so neither cost a consuming engine anything; `0.11.0` does not get
  that shelter — an engine implementing IR `0.10` **must widen its gate to
  `0.11`** purely to keep accepting artifacts, even though the artifact it
  receives is byte-identical in shape to the one it already reads. Per the
  `0.7.0` precedent (a minor move with no shape change still renames the
  schema file, because the file tracks the gated `major.minor`, not the
  release number), `schemas/lute-ir-0.10.schema.json` is renamed to
  `schemas/lute-ir-0.11.schema.json` (`$id` updated to match, body otherwise
  identical). A document stamped `luteVersion: "0.10.2"` now draws
  `W-LUTE-VERSION-STALE`; restamping to `"0.11.0"` is the whole migration.


## [0.10.2] - 2026-08-12

**A checked value stops evaporating at compile.** One change, entirely IR and
toolchain: a plugin-owned frontmatter key the checker already validates now
reaches the compiled artifact instead of being discarded the moment a checked
document becomes something a runtime reads.

### Changed

- **A plugin-owned, checker-validated frontmatter key now reaches the compiled
  artifact.** §6.8 (plugin-system `0.0.1`) let a plugin declare a top-level
  `meta` key with a schema, and `0.0.2` §3 made the checker enforce it
  (`E-FRONTMATTER-SCHEMA`) — both stopped at validation. `SceneMeta`/
  `QuestMeta` were closed structs and `artifact_meta`/`quest_meta` never read
  a plugin-owned key out of the raw frontmatter at all, so a document could
  pass the checker on a value that then evaporated at the one step that turns
  a checked document into something a runtime reads. Both envelope types gain
  a `plugin` object (`BTreeMap<String, Value>`, skipped when empty — a
  document authoring no plugin-owned key is byte-identical to before this
  change): a key counts only when it is BOTH declared by an active plugin
  (`snapshot.frontmatter`) AND its authored value independently passes that
  declaration's schema — `lute-compile` re-derives this from the snapshot
  itself rather than trusting a caller-supplied `CheckResult`'s `ok`, so a
  value the checker would reject can never leak into the artifact. Value
  conversion reuses the existing `Literal` → JSON path every `state:`
  `default:` already serializes through; nested record/map values stay
  key-sorted, so `meta.plugin` is deterministic at every depth. See
  [`plugin-system/0.0.4.md`](docs/proposals/plugin-system/0.0.4.md).
- **All three axes read `0.10.2`, and this time the IR earned it while the
  language did not.** `LUTE_IR_VERSION` and `schemas/lute-ir-0.10.schema.json`
  (`sceneMeta`/`questMeta` each gain a `plugin` property) catch up to the
  `meta.plugin` shape change in this same release, rather than lagging it —
  the schema file keeps its name and `$id` (it tracks the gated `major.minor`,
  which does not move) but its content does. Language `0.10.2` is
  byte-for-byte `0.10.1` semantics
  ([`scenario-dsl/0.10.2.md`](docs/proposals/scenario-dsl/0.10.2.md)).
  Documents carrying `luteVersion: "0.10.1"` now draw
  `W-LUTE-VERSION-STALE`; restamping is the whole migration.

## [0.10.1] - 2026-08-10

**A plugin is not a second-class citizen.** All three entries come from one
adoption project — a visual-novel prototype consuming `0.10.0` artifacts — and
all three are the same shape: a surface that works for `lute.core` and quietly
does less, or nothing, once a plugin is involved. None of them is a language
change; see *Not in this release* for the one that is.

### Fixed

- **`lute test` could not see a project at all.** The subcommand had no
  `--project` flag and passed `None` unconditionally, so a document that reaches
  its schema through a manifest's `defaults: uses:` or its directives through a
  `profile:` failed **every** test on `E-DOMAIN-UNKNOWN` / `E-UNDECLARED` /
  `E-UNKNOWN-DIRECTIVE`, no matter what the test asserted. Its sibling commands
  — `check`, `compile`, `trace` — all resolved the same manifest correctly, so
  `lute trace <doc> --project P` walked a document `lute test P` could not load:
  one question, two tools, two answers, which is the class `0.10.0` spent itself
  closing and this one missed. `lute test` now takes `--project` with `trace`'s
  flag, resolution order and provider-catalog precedence, and a project-
  resolution `E-` diagnostic gates the exit code instead of surfacing as a test
  failure. There is still no manifest auto-discovery — omitting the flag keeps
  the previous core-only resolution exactly.
  This is **not** backlog `#19`/`T9.7`, which is about `lute test` walking the
  source rather than the artifact and the derived-relation fixpoint. That one
  changes *what* the harness walks; this one is whether it can see the manifest.
  They are independent and neither blocks the other.
- **An `assetKind` segment could declare a type that enforced nothing.**
  `AssetSegment.ty` is the same shared `Type` enum every other typed position
  uses, so every variant parsed in a segment position while
  `validate_segments` enforced four of them and accepted the rest in silence.
  Measured, one plugin, one document, two segments: a segment typed
  `{ enum: [alpha, beta] }` given `NOPE` reported `E-ASSET-SEGMENT`; a segment
  typed `{ domain: … }` given `NOPE` reported nothing. The plugin spec's closed
  `Type ::=` production (plugin-system `0.0.1` §7) never admitted `domain` in a
  segment — it was reachable through a Rust enum, not by design — so the fix is
  to **reject the declaration** rather than to invent member validation the
  grammar does not describe. New `E-PLUGIN-ASSET-SEGMENT-TYPE`, at plugin load,
  naming the kind, the segment, the declared type and the four admitted ones.
  Every other inadmissible variant is rejected with it, each for a stated
  reason: `enumFromOption` and `slotId` are scoped "attribute types only" by the
  production itself; `narrativeTime` is opaque and never author-declarable;
  `list`, `record` and `map` have no serialization into the single delimited
  token a decomposed segment is; `assetKind` inverts the relation by describing
  a whole id rather than one token within one; and `bool`, though single-token,
  would recreate the identical declared-but-unenforced hole for a new variant.
  A domain used *only* as a segment also drew a spurious `W-DOMAIN-UNREAD`;
  rejecting the declaration removes that at the source.
- **Every plugin load and resolve error printed a Rust struct.** Both
  diagnostic sites built their message with `format!("{e:?}")`, so
  `E-PLUGIN-PARSE` reached the user as `Parse { file: "…", msg: "…" }` and the
  new code above would have shipped as
  `AssetSegmentType { file: "…", kind: "…" }`. `LoadError` and `ResolveError`
  now implement `Display` — one sentence per variant, in the voice the checker's
  own diagnostics use — and both sites render it. The structured fields were
  always there; only the rendering was missing.
- **`E-PLUGIN-ASSET-SEGMENT-TYPE` anchored at a directory.** It named the
  `assetkinds/` export directory rather than the `.yaml` carrying the
  declaration, because the merge callback only received the directory.
  `read_kind` now threads the per-file path to its callers.
- **One LSP test matched a diagnostic by its `Debug` text.**
  `analyze_publishes_project_resolver_diagnostics` located its target with
  `message.contains("DependsCycle")` — a substring of a Rust struct name — and
  so broke the moment that struct gained a `Display`. It keys on the stable
  `E-DEPENDS-CYCLE` code now, which is the doctrine the rest of the repo
  already follows.

### Changed

- **All three axes read `0.10.1`, and none of them earned it.** The alignment
  rule moves every visible axis on every release whether or not its contract
  changed, and this is the first release since `0.7.0` where the honest report
  is "no-op on two of three". `schemas/lute-ir-0.10.schema.json` keeps its name
  and its `$id`: the schema file tracks the gated `major.minor`, not the release
  number, which is why `0.7.0` renamed its schema and this release does not.
  Documents carrying `luteVersion: "0.10.0"` now draw `W-LUTE-VERSION-STALE`;
  restamping is the whole migration, and a `0.10.0`-clean document restamped to
  `0.10.1` checks clean with no other edit.

### Not in this release

- **The staging reducer still dispatches on the literal source tag.** A plugin
  directive declaring `lower: { record: background }` gets none of the stage
  semantics its record implies: the core `::bg` injects a sprite exit at a scene
  change and the plugin equivalent injects nothing, so an engine consuming the
  second artifact leaves a character on stage. Two further rules diverge in two
  further directions — an injection silently dropped, and a `posReset`
  fabricated for a character no longer in the scene. It is filed rather than
  fixed because flag-driven dispatch has already been declined twice on the
  record (`plugin-system/0.0.3.md` §4, `scenario-dsl/0.9.0.md` §7) against a
  semantics vocabulary that genuinely cannot drive it — `mutatesScene` is shared
  by `::bg` and `::music`, so branching on it would make music clear the stage.
  Closing it needs a new closed flag or record-intrinsic dispatch, and either
  changes what the checker emits about a legal document, which puts it on the
  language axis. Evidence, reproductions and both remedy shapes:
  [`2026-08-10-staging-tag-dispatch.md`](docs/superpowers/notes/2026-08-10-staging-tag-dispatch.md).
  Also filed there: `lower:`'s own grammar is written closed in
  plugin-system `0.0.1` §8.2 and parses open, so a misspelled key — including
  one belonging to the sibling untagged variant — is dropped in silence.

## [0.10.0] - 2026-08-06

**The toolchain says what it knows.** Every entry below is a place where the
tool already held the answer and did not use it: it resolved a type and did not
apply it, held a permitted-attribute table and enforced one row of it, proved a
relation dead and reported that in one slot and nothing in another, computed a
layer and rendered none. Once, it said the opposite of what it knew.

`0.10.0` was scoped from a drive test: eighteen documents written *in* Lute on
purpose, producing a 111-entry findings log and a 38-issue backlog, of which
this release takes twenty-six. Specs:
[`scenario-dsl/0.10.0.md`](docs/proposals/scenario-dsl/0.10.0.md) — thirteen
language changes, six `LANG` and seven `LANG-SOFT`. `LUTE_LANG_VERSION`,
`LUTE_IR_VERSION` and the toolchain version all read `0.10.0`; the IR schema is
[`schemas/lute-ir-0.10.schema.json`](schemas/lute-ir-0.10.schema.json) and this
time the shape **moved** — see the IR bullet under *Changed*.

### Changed

- **BREAKING (IR) — `provenance.reason` is now `provenance.explanation`.** On
  the injection provenance stamp an artifact carries for every command the
  compiler synthesized:
  `{ "injected": true, "by": "auto-pose-reset", "explanation": "…" }`. The old
  name was a **collision, not a synonym**. `end.reason` is an opaque author
  token a host dispatches on — the author writes it and your engine branches on
  it. This field is human-readable English the compiler wrote to say why a
  record you did not author exists, and nothing dispatches on it. Two keys
  sharing a name with nothing else in common is exactly what a rename removes.
  An engine gated on IR `0.9` **must widen to `0.10`**, because the runtime
  contract requires refusing a newer major.minor; **the rename is the only edit
  it needs** beyond that. `provenance.injected` is retained but is now
  constant-`true` — with `W-INJECT-CONFLICT` gone nothing can construct a
  `false`, so do not read a `true` as distinguishing anything. Removing the
  field would be a second IR break and is deferred.
- **BREAKING (documents) — `::set` now checks the value it writes against the
  path it writes to.** `::set{run.shedPressure += "two"}` where the schema
  declares `{ type: number }` is `E-SET-TYPE`, at the right-hand side's own
  span. Every report is a write the runtime was already discarding: `+= "two"`
  on a number left the path at `0`. The checker had resolved the target's
  declared type all along — it used it to diagnose a *different* construct in
  the same run, on the same path, and never applied it to the write. It remains
  a proof obligation, never a guess: an expression whose type cannot be decided
  is accepted silently rather than guessed at.
- **BREAKING (documents) — an attribute the logic tags do not accept is now an
  error.** `<branch>`, `<choice>`, `<match>`, `<when>`, `<otherwise>` and
  `<hub>` all close their attribute sets, and a name outside the set is
  `E-UNKNOWN-ATTR` at the attribute's own column. It was already being
  discarded — silently, which is why a typo'd `when=` on a `<choice>` produced
  an unguarded choice and no complaint. Only `<otherwise>`'s empty set was
  enforced before, out of the same table the other five never consulted.
  `<choice>`'s set is **position-dependent**: `once` and `exit` are hub-choice
  only, so `exit` on a branch choice, which the hub reducer is the only reader
  of, no longer passes in silence. And `as=` on a `<choice>` is its own
  `E-AS-REMOVED` rather than "unknown", because it is not unknown — it was
  renamed to `into=` in `0.1.0`, `lute fix` performs the rename, and doing so
  restores the `set` record the document was losing.
- **BREAKING (documents) — a quest gate that can never open is an error.** A
  `<quest start=>` querying a relation nothing can ever produce is
  `E-QUEST-UNREACHABLE`, naming the relation and the declared routes. The
  producibility fixpoint already proved it: it reported the identical fact in
  `done=` as a project-wide error and in `start=` as nothing at all, after
  which `scenario reach` printed **Reachable** for the silent one. It fires on
  `start=` only, never on `fail=` — a `fail` that can never hold means the
  quest cannot fail, which is not a defect.
- **BREAKING (documents) — two required objectives that cannot both hold are an
  error.** `done="run.shedPressure >= 99"` and `done="run.shedPressure <= 0"`
  on one quest is `E-OBJECTIVE-CONTRADICTION`, naming both ids and the path.
  The diagnostic names both because it cannot know which one is wrong. Scoped to
  path-versus-literal scalar comparisons, and it carries the "this quest can
  never complete" consequence as a note rather than escalating to a second
  diagnostic.
- **BREAKING (mocks) — `mocks/*.yaml` requires a `file:` key, and is now
  checked.** `file:` names the document the mock previews, resolved **relative
  to the mock**; a mock without one is `E-MOCK-SUBJECT`. There is no
  subject-less mode. This is what makes the rest possible: `check-project` now
  validates every `mocks/*.yaml` it walks, so a mock seeding an undeclared path
  or naming a choice id that no longer exists is reported by the ordinary
  project check instead of only when someone happens to run `lute trace`. Mock
  diagnostics anchor at the mock file and name the offending key in the message
  — no line and column, because spanned YAML is not in scope. When
  `lute trace <doc> --mock m.yaml` disagrees with `file:`, the command line wins
  and the disagreement is the error.
- **BREAKING (`*.test.yaml`) — the key set is closed, and a test that asserts
  nothing fails.** Unknown keys were dropped at both nesting levels, so a file
  spelling `chooses:` lost its selection, `trace` auto-picked the first
  eligible arm, and the assertions written for the arm the file *names* were
  checked against the arm it excluded — green. Both levels now close with the
  same edit-distance did-you-mean four checker codes already use
  (`E-TEST-KEY`). And the verdict was `all()` over an empty vector, so a test
  with no recognised expectations reported **PASS**; that is now
  `E-TEST-NO-EXPECT`. Auto-picking a branch stays legal and stops being silent:
  every auto-picked branch is named along with the arm it took.
- **BREAKING (`lute run`) — a forced selection whose guard decided false is
  refused.** Asking for a choice arm whose `when=` evaluates false played it in
  full at exit `0` — in the drive test, a character delivered four lines from
  inside a cryopod. `lute trace` refused the same selection on the same
  document in the same project, and `lute test`, being trace-based, inherited
  the refusal: one question, three tools, two answers. The guard was already in
  the artifact as `option.when` and this walk already evaluated CEL everywhere
  else in it. Hard refusal, exit `2`, no opt-in flag — a flag to keep the old
  behaviour would re-create the disagreement under another name. Covered on both
  dispatch sites; a hub option that a prior visit's `::set` enabled still plays,
  because the hub evaluates per visit.
- **BREAKING (`loc export`) — a component's lines are exported once per call
  site, under the caller's id.** Adopting the language's only reuse mechanism
  used to remove a line from the localization pipeline with no diagnostic
  saying so: the export keyed a component's lines to the *component* file with
  `lineId` null, because `{prefix}` derives from the importing document's
  frontmatter and a component has none. Everything downstream keys on `lineId`,
  so `loc import` skipped the row at exit `0`, `lute tag` answered "already
  tagged" (the lines *do* carry a `code=`), and `compile --locales` then emitted
  `W-L10N-MISSING` for a caller-derived id that appeared in no export the
  translator ever saw — and shipped English at exit `0`. The export now
  normalizes first, the same pass `trace` and `compile` run, so each line is
  extracted once per call site with the caller's prefix and its `@params` bound.
  A new `source` field carries the component file and line so a TMS can dedupe
  identical text. `{{…}}` interpolation is deliberately left intact — that is
  what a translator must see.
- **Timeline time is integer milliseconds.** `at`, `duration` and `delay` are
  authored exactly as before, and the checker now converts each by **shifting
  the authored decimal**, never by multiplying a parsed float, so overlap and
  duration comparisons are exact. A boundary hand-off — `at="0.8"
  duration="0.4"` then `at="1.2"` — is legal, as the spec always said and
  floating-point accumulation denied; the epsilon and the shortened-duration
  workarounds authors wrote to dodge `E-CLIP-OVERLAP` can be deleted. A value
  finer than a millisecond is `E-TIME-RESOLUTION`. `E-CLIP-OVERLAP` and
  `E-TIMELINE-DURATION` print the **authored** decimal, never a reconstructed
  float. **The artifact keeps seconds** under the same names and JSON type — a
  cursor-derived `1.2` simply stops serializing as `1.2000000000000002`.
  Renaming them to milliseconds would place every effect 1000× late in an engine
  that did not notice.
- **A standalone component check no longer contradicts the project one.**
  `lute check c.component.lute --project P` said `ok` for a component that
  cannot work with *any* of its callers, while `check-project` reported the
  fault once per caller at line 1 of the wrong file and `lute trace` refused
  with "run `lute check` first" — advice that could not be followed. Four
  changes, one contract: a component-body diagnostic keeps its
  component-internal line and column as a secondary location instead of
  collapsing onto the importer's frontmatter span; identical reports across N
  callers roll up to one, with `(+N more callers)`; a malformed `params:` is
  reported as `E-COMPONENT-PARSE` on the standalone leg and the
  `E-UNDECLARED-REF` it *causes* is suppressed, so the author is no longer sent
  to `defs:` for a param they declared four lines up; and with at least one
  caller in scope the standalone leg reports what holds at **every** call site,
  anchored inside the component. A fault holding at only some sites is
  caller-specific and stays with `check-project`, where the caller is visible.
  With **no** caller in scope the verdict is `W-COMPONENT-UNVERIFIED`, not `ok`
  — refusing to claim a check it did not perform. "No caller in scope" covers
  both of its disjuncts, including the one an author actually types: `lute
  check c.component.lute` with **no** `--project` (there is no manifest
  auto-discovery). The two disjuncts do not share a message — "no project
  resolved" means the tool could not look, "no document imports this" means it
  looked and found nothing.
- **`lute check` runs the compile gate, so it stops being greener than
  `trace`.** `normalize` + `expand` run after the `check` gate in both
  `lute compile` and `lute trace`, and `lute check` ran neither: a scene whose
  `defs:` bodies form a cycle reported `ok: … (0 warning(s))` while `lute trace`
  on the same file printed `E-COMPILE-EXPAND … def expansion cycle: a -> b -> a`
  and then *"has check error(s) — run `lute check` first"*. That advice was
  unfollowable by construction for the whole `E-COMPILE-*` class. `lute check`
  now runs the same two passes, in the same order, past the same gate, and
  reports what they find; `E-COMPILE-COMPONENT`, `E-COMPILE-EXPAND`,
  `E-COMPILE-INTERNAL` and `E-WHEN-UNSET-SUBJECT` join the `--deny` universe
  accordingly.
- **A component is not a root document.** `lute trace`/`lute compile` on a
  `*.component.lute` used to fail with the expander's own internal invariant
  assertion — `` `@pressure` names no known def body (gate should have caught
  this) `` — and blame `check`, which reported that exact file `ok`. A
  component's `params:` are bound at each `::use`, so it has no standalone
  compiled form and no standalone walk; both commands now refuse the invocation
  for that reason and point at an importing document. On the `check` side the
  gate binds the params as a call site would, so a component's own body faults
  are reported while the absence of a caller is not mistaken for one.
- **`&&` narrowing runs in every CEL slot.** `<quest start|fail>` and
  `<objective done|when>` were the four slots where an intra-expression
  `x != unset && x > 3` did not discharge `E-MAYBE-UNSET`, as it already did
  everywhere else.
- **A component param may be declared `{ type: X }`.** Accepted as a synonym
  for the short form, so the long form authors reach for by analogy with
  `state:` and `defs:` no longer fails.
- **`--coverage` keys a `<match>` on its position, not on its guard text.** Six
  blocks opening `<match on="true">` across four files collapsed into one row
  reading `3/3 arm(s) executed` — the tool's only false statement, and its most
  reassuring one, certifying a set of six blocks no single traced path ever
  visited together. A `<match>` is now keyed on file plus line/column with the
  guard text riding along as a label (a `<branch>`/`<hub>` keeps its declared
  id, which is document-unique). The same run over the drive-test corpus renders
  19 match rows where it rendered 10. Coverage also used to accumulate only
  from reports that *ran*, so deleting a test made its scene invisible rather
  than untested; `--coverage` now lists every testable document no
  `*.test.yaml` names.
- **`E-CEL-PARSE` inside a `::set` body names `::set`'s own attribute surface**
  and drops the `'=' assigns; comparison is '=='` suggestion, which is advice
  for a guard and wrong for an assignment.
- **`E-MAYBE-UNSET` on `quest.<id>.state` names a remedy that exists.** It used
  to prescribe definite assignment, which is not reachable for a reserved
  quest path; it now names the two forms that do work.
- **`E-LOGIC-CONTENT` loses its attribute arm.** An attribute on `<otherwise>`
  is now `E-UNKNOWN-ATTR`. The code is unchanged for its three body-shape
  rules. This is the one message change on a construct that already enforced.
- **A nested `lute.project.yaml` is validated by every command that walks it.**
  `compile` and `compile --all` reached nested manifests and never validated
  them, so a broken one that `check` rejected compiled at exit `0`;
  `E-IDENTITY-TEMPLATE` and its siblings now fire from `compile` too and carry
  the manifest's own path, which they did not before. A nested manifest that the
  invoked root does **not** govern draws `W-PROJECT-INERT` — but only when it
  would have resolved a different capability snapshot, different identity
  templates, or a different `defaults:` block, because an unconditional
  warning fires on manifests whose presence changes nothing. Those three are
  exactly what a document resolves through its governing manifest, so a
  nested root cannot be inert in a way that matters and stay quiet: two roots
  supplying different `season:`/`character:` defaults rewrite every `lineId`
  in the inner subtree with both `check-project` and `compile --all` at exit
  `0`.

### Added

- **`defaults:` in `lute.project.yaml`.** Hoist frontmatter every document in a
  root repeats — `character`, `season`, `profile`'s neighbours, `uses:`,
  `extends:` and the rest of a closed defaultable set — into the manifest, and
  let a document override any of them. Purely additive; nothing requires it.
  Override is **whole-value per key**, never merged, so a document that names a
  key owns that key outright. A key outside the set is `E-DEFAULTS-KEY`: schema
  keys already compose through `uses:`/`extends:`, `profile` and `plugins`
  already have manifest routes, and `title`/`episodeId`/`after` are per-document
  by nature. `mode` is excluded because it is inert — a legal key nothing reads,
  and a defaultable key that changes nothing is a trap in a block whose whole
  purpose is changing many documents at once. A `uses:`/`extends:` path in
  `defaults:` resolves relative to the **manifest**; the same key in a document
  resolves relative to the **document**. The rest of the manifest stays open;
  only the `defaults:` mapping is closed.
- **`W-DOMAIN-UNREAD` — a declared domain nothing reads.** Project-wide only
  (`check-project`, never single-document `lute check`), because a domain
  declared in a shared schema is read by *some* document and warning on the
  scene that happens not to read it would be a false positive on the most
  common layout in the language.
- **`W-EXIT-INERT` and `W-STAGE-ABSENT` — stage state the reducer already
  held.** A content-line `action=` naming a member of the `action` domain's
  declared `exits:` looks like it removes the character and does not — only
  `::auto` does — so it is `W-EXIT-INERT`, and the message names both discharge
  paths: split it into the two-event form, or stop declaring that member an
  exit. A staging event on a character already removed by an explicit declared
  exit is `W-STAGE-ABSENT`, firing only after such an exit and only until a
  re-show, so a character's first line — which legitimately puts them on stage
  — never warns. Two codes rather than one, because they are different claims
  and `--deny <CODE>` must be able to separate them.
- **`E-RELATION-UNKNOWN` suggests the nearest declared relation.** State paths,
  `after:` scene keys, `::set` targets and enum members all offered a spelling
  suggestion; relation names were the one class that did not, against a
  `relations:` block that is the cheapest closed set in the language to compare
  against. In one drive-test run `run.shedPresure` got its suggestion and
  `can_hlat`, two lines up, got nothing. Deterministic tie-break; the
  entity-kind hint keeps precedence, so a name that *is* a declared kind still
  gets the categorical explanation rather than a spelling guess.
- **`E-META-UNKNOWN-KEY` suggests the nearest known frontmatter key.**
- **`lute trace` renders the exit, the ending, and the heading.** An exit was
  invisible: an entrance and an exit are the same construct with the same
  attribute names, and the entire difference is which value appears in the
  `action` domain's declared `exits:` — `trace` printed both as `<auto>`, and
  now prints `<auto exit>`, read from the resolved domain, never inferred. A
  terminator was unlabelled: `reason` is `::end`'s entire payload, the only
  thing distinguishing it from falling off the end of the document, and a
  project with several endings previewed them all as an identical `<end>`;
  it now prints `<end reason=bridge-reached>`. `TraceReport` gains `disposition`
  and `endReason` as additive keys, so a harness can finally tell a terminated
  walk from a spent one.
- **`scenario envelope` reports the computed layer, not the declared one.** The
  join existed and was rendered nowhere: the assembly pass built per-scene write
  sets and per-quest completion writes, handed them to propagation, and dropped
  them. Inverting that names the **writers** of every path — the edge nobody
  could draw. Scoped to graph ancestors rather than project-wide, because a
  project-wide list renders identically at every node and would name a scene
  eight scenes downstream as a writer in an earlier scene's pre-entry envelope.
  Both halves are reported and each is labelled with what is actually known:
  writers on a declared route reaching the node, and writers whose write is not
  provably before it.

### Fixed

- **A refused test prints the diagnostics it is holding.** A `choose:` naming a
  deleted choice id, one naming a deleted branch id, and a `state:` naming a
  deleted path all produced the same single line, `trace refused: invalid mock
  input`. The harness was holding the diagnostic vector, *inspecting the codes
  in it* to pick between two canned strings, and then discarding it. On a
  31-file suite that is the difference between a one-second fix and a bisect.
  Flag spellings are rewritten to key spellings on the way out, because a
  `*.test.yaml` cannot use `--choose`.
- **A malformed imported state declaration is named instead of counted.**
  `E-USES-PARSE` reported `(1 issue(s))` and nothing else — the author's total
  information about a four-word mistake in a schema. It now carries the
  import's own diagnostics as `related`, positioned against the imported file,
  through the renderer that already walks `related`; the count is computed from
  that same vector so the two cannot disagree. And `lute check world.schema.yaml`
  — the obvious next command — used to parse the YAML schema **as a scene** and
  tell the author to add `kind: scene`, which destroys it; a `.yaml`/`.yml`
  target now takes the same schema lift it gets when reached through `uses:`.
- **A content-line enum error names the line, not an invented directive.** The
  enum-member check hardcoded the `::` directive sigil and content lines passed
  it the *speaker*, so every content-line enum error named a `::narrator` that
  exists in no document, no grammar, and no `lute context` listing. Directives
  render `::auto`; content lines render `@narrator`. `E-ATTR-TYPE` had the same
  defect through the same call path and is fixed with it. Separately,
  `scenario envelope` on a quest annotated an author-facing table with an
  internal task label and a Rust function name; the distinction it draws is real
  and kept, in the author's vocabulary.
- **Nine false documentation statements, rewritten from what the binary
  prints.** Among them: `when=` was described as unqualified sugar for a one-arm
  `<match>`, when a relational guard is legal on a line and
  `E-MATCH-RELATION-SUBJECT` as a subject — where the guard queries facts the
  line form is the *only* form; "quest documents additionally use
  `::assert`/`::retract`", which a scene in the corpus disproves; and
  `{ type: enum, values: [...] }`, copied into three files, which is
  `E-STATE-DECL`. A gap you can see costs a workaround; a false sentence costs
  rounds you do not know you are spending.
- **Three documentation silences broken**, each anchored in runnable output
  rather than written from a plan: a worked `identity:` block naming the two
  templates *as* the defaults a project gets without declaring them and stating
  the two id classes they do not govern; what a content line's `action=`
  actually does (it sets the pose and marks the speaker dirty, so the next plain
  line gets an injected pose reset) and that an entrance and an exit are
  `::auto`; and that a quest's `after` is an **attribute** on `<quest>`, said on
  the page that owns both document kinds.
- **The `--deny` code registry documented its own guard in the wrong place.**
  Three documents named `crates/lute-cli/tests/deny.rs`, following a doc comment
  in `main.rs`; the drift guard is a unit test inside `main.rs` itself. The
  comment and all three documents are corrected — the misdirection had already
  misled two independent readers.

- **A component file checked standalone now enforces the presentational-body
  contract (dsl 0.4.0 §6.2).** `lute check some.component.lute` reported `ok`
  for a body containing `<branch>`, `<hub>`, `<timeline>`, `<on>`,
  `<objective>`, `::set`, `::assert`, or `::retract` — every one of which fails
  with `E-COMPONENT-BODY` the moment the component is reached through a
  `::use`. A component file carries no `kind:`, so it degrades to
  `DocKind::Scene` and walked through the ordinary scene `Walker`, where all of
  those constructs are legal; `walk_component_body`, which owns the
  prohibition, was reached only from `validate_components` over an *importing*
  document's component table. The standalone leg is the one a component author
  is most likely to run, and it was a false green. The component root now walks
  through the same `walk_component_body` the `::use` leg uses — one
  implementation of the contract, not two. This is the mirror of the earlier
  fix that made the standalone leg no longer too *strict* about a component
  file's own `<quest>`.

  A `<hub>` additionally draws `E-HUB-NO-EXIT` on the standalone leg only,
  because the branch-folding pre-pass runs over any root document; that
  residual is deliberate and documented in
  `crates/lute-check/tests/component_logic_block.rs`.

- **`docs/examples/components/greet.component.lute` and
  `showcase/components/stinger.component.lute` documented a rule the language
  dropped.** Both header comments stated dsl §13.4's blanket ban on logic
  blocks in a component body; 0.4.0 §6.2 has admitted a param-scoped
  `<match on="@param">` since then, which `reaction.component.lute` relies on.
  Reading the examples taught the wrong rule. `stinger`'s claim that a
  standalone check reports `E-META-MISSING` was also stale — it reports
  `E-DOMAIN-UNKNOWN`, because that file declares no `uses:` of its own.

### Removed

- **`W-INJECT-CONFLICT`.** The first removal in this series, and the case that
  gives the release its second clause: **the toolchain said the opposite of what
  it knew.** The warning fired on `anchor="center"` where `center` is the
  declared default — and *only* there. Writing a different anchor was silent;
  writing none was silent. The one authored shape it complained about was
  **agreement**. The injecting rule only injects in the no-anchor arm, so a real
  conflict is structurally impossible; narrowing the code to "and the values
  differ" makes it unsatisfiable, which is why this is a removal and not a
  narrowing.

  **The information it carried is dropped, not migrated.** It was the only
  record that an author wrote what a rule would have injected, and there is no
  `injected: false` provenance surface to fall back on — no such surface has
  ever existed, and building one would plant a spurious anchor record in the
  artifact. An earlier draft of the spec claimed otherwise; that claim is
  retracted. If you were consuming this warning, it is gone and nothing replaces
  it.

  `--deny W-INJECT-CONFLICT` is now a usage error, exit `2`, because the code
  left the deniable registry with it.

## [0.9.0] - 2026-07-29

**Language `0.9.0` — vocabulary ownership: the core declares slots, the project
declares members.** Breaking at the language axis (pre-1.0 allowance). Specs:
[`scenario-dsl/0.9.0.md`](docs/proposals/scenario-dsl/0.9.0.md) and
[`plugin-system/0.0.3.md`](docs/proposals/plugin-system/0.0.3.md).
`LUTE_LANG_VERSION` is `0.9.0` and the toolchain ships as `0.9.0`.
**`LUTE_IR_VERSION` also moves to `0.9.0`** under the axis-alignment rule even
though the artifact shape is untouched; the IR JSON schema is
[`schemas/lute-ir-0.9.schema.json`](schemas/lute-ir-0.9.schema.json), the `0.8`
file renamed with no shape edit. **For a consuming engine the IR bump is a
no-op apart from one gate widening** — see the IR bullet under *Changed*.

### Changed

- **BREAKING — a document must declare the content vocabulary it uses.**
  `lute.core` ships **no vocabulary members**. It declares seven *slots* —
  `emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`, `vfxType` — as
  the types of core content-line and directive attributes, and nothing more.
  Every member now comes from one of three declaration routes: an `enums:` block
  in the using document's **own frontmatter**, a project schema's `enums:`
  (imported through `uses:`/`extends:`), or a plugin's `enums` export. Using a
  slot that no source declares is `E-DOMAIN-UNKNOWN`, and the diagnostic names
  all three routes.

  Until now those six baseline vocabularies were closed lists inside
  `lute.core` that **no route could extend**: a project schema declaring
  `emotion:` got `E-DOMAIN-DUP` and had its members dropped, a project's own
  capability plugin exporting `emotion` failed whole-project resolution with
  `E-PLUGIN-DUP-ACROSS`, and `lute.core` cannot be deactivated. Measured against
  one real catalog, **20.7% of 30,861 authored `emotion` values were
  unrepresentable**.
- **`action` is now validated.** It previously carried a guard that *skipped*
  validation whenever nothing declared the domain, so 9,880 values across 53
  distinct ids received no checking at all and a typo like `step-foward`
  shipped. The guard is gone; `action` behaves exactly like `emotion`.
- **`::auto{action}` and `::music{mood}` are domain-typed**, having been free
  strings. This is why the `mood` domain had been declared-but-inert since it
  shipped.
- **An `::auto` that omits `anchor` now checks the `anchor` slot it implicitly
  reads.** The default-anchor injection reads the `anchor` domain's `default:`,
  but nothing in the document names `anchor` on that path and directive
  validation only sees AUTHORED attributes — so a project that declared `action`
  and forgot `anchor` checked clean while the anchor command 0.8.0 emitted
  unconditionally simply disappeared from the artifact. An undeclared slot is now
  `E-DOMAIN-UNKNOWN` there too, reported on the `::auto` itself, so the slot rule
  above holds for implicit reads as well as written ones. Writing an explicit
  `anchor` was, and remains, an error at the attribute.
- **A component body is checked the same way through `::use` as standalone.**
  Five whole-document passes ran only at the document root and never over an
  imported component body, so the same content checked clean inside a component
  and dirty at scene level. All five now run over component bodies: content-line
  attributes (`E-DOMAIN-UNKNOWN`, `E-BAD-ENUM`, `E-UNKNOWN-ATTR`, the delivery
  rules), duplicate line codes (`E-DUP-LINE-CODE`), reachability (`E-ARM-DEAD`,
  `W-CODE-AFTER-END`), admission of a component's unwalked top-level content
  (`E-GRAMMAR-NOT-ADMITTED`), and injection folding (`W-INJECT-CONFLICT`). Two
  of the five let real defects reach the artifact: an undeclared vocabulary
  value, and a duplicated `lineId`. A third silently **dropped** a component's
  top-level `<quest>` entirely. **A component body that used to check clean may
  now report; every such report is a defect that was already there.**
- **IR `0.9.0` — the number moves, the shape does not.** `irVersion` is now
  `0.9.0` and the schema is
  [`schemas/lute-ir-0.9.schema.json`](schemas/lute-ir-0.9.schema.json), the
  `0.8` file renamed. **No field is added, renamed, or moved, and no command
  `kind` is new**: IR `0.9.0` is shape-identical to IR `0.8.0`, and the number
  moved only because a release re-aligns every axis. **This is a no-op for
  consumers except for one thing, which is not optional**: the
  [runtime contract](docs/runtime/execution-model.md#version-negotiation)
  requires an engine to refuse an artifact from a newer `irVersion`
  major.minor, so an engine implementing `0.8` **will reject every `0.9.0`
  artifact** until it widens its gate to accept `0.9`. Widening the gate is the
  whole migration — no parser change, no new field, no new behaviour.
- **Artifact content changes; the artifact shape does not.** A project-declared
  vocabulary — inline or imported — now reaches the compiled artifact's existing
  `enums` array, because it is project data like `entities:`/`relations:`. A
  vocabulary supplied by a plugin `enums` export does not appear there — it is
  part of `capabilityVersion`.
  `capabilityVersion` changes for every project (the core's vocabulary emptied
  and two attribute types changed).
- **`capabilityVersion` covers the member semantics, not just the members.** The
  stamp folds a plugin-exported vocabulary's `default:`/`exits:` alongside its
  member list. Those keys now decide emitted output — the injected anchor and
  `sprite.exit` — so two capability surfaces that agree on members and differ
  only there compile differently and no longer share a stamp. A surface carrying
  no vocabulary at all hashes byte-identically to before.
- **`lute new scene`** imports `vocabulary.schema.yaml` when the project has one.

### Added

- **`enums:` long form** — `{ members, default, exits }`. A bare list stays
  shorthand for `{ members: [...] }`, so every existing declaration keeps parsing
  byte-for-byte. A declaration of `action` **MUST** supply `exits:` (the members
  that exit their character) and a declaration of `anchor` **MUST** supply
  `default:` (the member used when the attribute is absent); for the other five
  slots both keys are rejected. Four new diagnostics:
  `E-ENUM-MISSING-SEMANTICS`, `E-ENUM-UNEXPECTED-SEMANTICS`,
  `E-ENUM-DEFAULT-NOT-MEMBER`, `E-ENUM-EXITS-NOT-MEMBER`. The same validator
  runs on all three routes.
- **A document's own inline `enums:`/closed `entities:` now declares vocabulary
  for that document.** `enums:` has always been legal frontmatter in any
  document, but the projection was built and then dropped before the domain
  merge, so using what you had just declared on the line above still reported
  `E-DOMAIN-UNKNOWN`. It now reaches the merge by the same path an imported
  declaration does, and surfaces in `lute context --json`'s `projectEnums`, in
  `lute doctor`'s slot report, and in LSP hover/completion. This is the only
  route open to a single-file author or the playground, which checks one
  in-memory document and can resolve no import. Precedence: inline wins over an
  imported declaration of the same slot and must re-declare a superset of its
  members (`E-EXTENDS-RELATION-SIG` otherwise); against a plugin or the core it
  is `E-DOMAIN-DUP` and the plugin wins. A component body is the one place it
  does not apply — see *Known limitation*.
- **`lute init` scaffolds a starter vocabulary** (`vocabulary.schema.yaml`)
  covering all seven slots with `exits:`/`default:` filled in, so a fresh project
  checks clean out of the box and its starter scene actually uses a slot. The
  opinionated default lives in the template, not in the compiler.
- **`lute doctor` reports vocabulary slots**, with the member semantics inline:
  `vocabulary slots declared: emotion, action (exits: …), anchor (default: …), …`.

### Removed

- **The hardcoded exit heuristic**, in *both* hand-synced copies (the checker's
  reducer and the compiler's lowerer, the second commented "mirrors … byte-for-
  byte"). Exit is now membership in the declared `exits:` list. Gated on a table
  test proving the new reading reproduces the old verdict over the full fixture
  corpus before either copy was deleted.
- **`DEFAULT_ANCHOR = "center"`** — replaced by the declared `anchor`
  `default:`. Production code now branches on **zero** domain members.
- **Two `semantics` flags with no consumer** — `isStateful` and
  `cancelsPrevious` (plugin 0.0.3 §4). The closed vocabulary goes from twelve
  flags to **ten**; no shipped plugin declared either.
- **Dead `pose` attribute reads** in the stage reducer. `pose` is not a known
  content-line attribute, so `@x{pose="…"}` was already `E-UNKNOWN-ATTR` and
  neither read was reachable.

### Migration

1. **Declare your vocabulary**, by whichever of the three routes fits. Add an
   `enums:` block to a schema your documents already import (best for a
   multi-document project), add one to a single document's own frontmatter (the
   only route open to a one-file author or the playground, which resolves no
   imports), or export `enums` from your own capability plugin. `lute init`
   scaffolds one; `lute doctor <dir>` lists which slots a project root has
   declared, and `lute check` names all three routes on the first undeclared use.
   Declare per project root — a sibling root's declaration does not reach in.
   Declaring one slot through a plugin **and** either project route in one root
   is `E-DOMAIN-DUP` (the plugin wins); declaring it both inline and in an
   imported schema is not, but the inline block must re-declare a superset of the
   imported members or it is `E-EXTENDS-RELATION-SIG`.
2. **Spell out the member semantics** — `exits:` for `action`, `default:` for
   `anchor` — and include the members the old core rejected.
3. **Restamp** `luteVersion: "0.8.0"` → `"0.9.0"` (the pre-existing
   `W-LUTE-VERSION-STALE`).
4. **Fix what the component bodies were hiding** (see *Changed*).

`conformance/` needs **zero** fixture edits: no conformance source uses any of
the seven slots.

#### Known limitation

A component body resolves its vocabulary against the **importing** document,
because neither a component's own `uses:` nor an inline `enums:` block in its
frontmatter is carried through `::use`. So a component naming a domain only *it*
declares passes a standalone `lute check` and fails through a `::use` from a
scene that does not declare or import the same vocabulary. Keep the
declaration at the project root so both reach the same one. A *component schema*
surface that carries a component's own imports into the expansion is a named
future direction, filed separately
([`scenario-dsl/0.9.0.md`](docs/proposals/scenario-dsl/0.9.0.md) §6.1).

## [0.8.0] - 2026-07-27

The **adoption release**. Every item here traces to a concrete gap found while
assessing Lute against a real, large game catalog (hundreds of authored
scenes, tens of thousands of command rows; the assessment itself is not
included in this repo).
Specs: [`scenario-dsl/0.8.0.md`](docs/proposals/scenario-dsl/0.8.0.md) and
[`plugin-system/0.0.2.md`](docs/proposals/plugin-system/0.0.2.md).

All three version axes advance to `0.8.0`; the IR JSON schema is renamed
`schemas/lute-ir-0.7.schema.json` → [`schemas/lute-ir-0.8.schema.json`](schemas/lute-ir-0.8.schema.json).
A document stamped `luteVersion: "0.7.0"` fires the pre-existing
`W-LUTE-VERSION-STALE`; restamping is the only edit a 0.7.0-clean document
needs (see *Changed* for the one exception).

### Fixed

- **`addr` field width no longer overflows** — the index segment was fixed at
  4 digits, so a shot with 100+ records emitted `001-11500` beside `001-1400`
  and **lexicographic ordering silently diverged from execution order**; an
  engine that ordered or range-checked addresses as strings would rewind into
  already-played content. This was hit in production by the `tactus` pilot and
  was invisible to the conformance suite, whose fixtures were all 4-digit.
  Both segments are now padded to a width computed from the document and
  **uniform across the whole artifact**, so *lexicographic order over `addr`
  equals execution order* is a guarantee an engine may rely on. The fold counts
  only addresses actually emitted, so a document whose every shot emits fewer
  than 100 addresses is byte-identical to 0.7.0.

### Added

- **`::end{reason?}`** — the ninth `lute.core` directive and a new IR command
  kind `end`: terminate the walk, carrying an optional free-form reason the host
  may surface. Content after an `::end` in the same straight-line body is
  reported `W-CODE-AFTER-END`. New conformance fixture `conformance/end-reason/`.
  Termination is control flow, so it is core rather than a plugin directive —
  a plugin record is opaque to reachability analysis and could not be proven to
  terminate.
- **`after:` gains `active("questId")`** — the prerequisite profile admitted
  `visited` and `completed`, but the quest lifecycle is
  `unset → active → complete|failed`, so it could express two of three
  observable states. Graph semantics match `completed` (reachability, cycles);
  the state envelope is strictly weaker. `lute scenario` reports the edge kind
  in `text`, `json` (`kinds`), and `dot` (`active` renders dashed).
- **`quest.<id>.activatedAt`** — a reserved `narrativeTime` slot the engine
  stamps at the `unset → active` transition. `validAt(rel, t)` existed since
  0.3.0 but had **no author-writable `t`**; this is it. Readable in CEL,
  never author-declarable (`E-QUEST-RESERVED-DECL`) or writable
  (`E-QUEST-RESERVED-WRITE`), and exempt from `E-MAYBE-UNSET` because a
  maybe-unset verdict on it would be undischargeable.
- **`Artifact.shots`** — authored `## ` headings now survive compilation.
  0.6.0 made shot headings free text and lowering discarded them, so a
  compile → decompile round trip lost every section title; headings were the
  only authored structure with no other IR carrier.
- **Localization round trip** — `lute loc import <file>…` canonicalizes
  `loc export` output into a `lineId`-keyed locale bundle, and
  `lute compile --locales <bundle.json>` merges it into `LineCmd.texts` and the
  choice/hub option `labels`. `text`/`label` stay the source-language string, so
  a 0.7 consumer is unaffected. A missing `(lineId, locale)` pair is
  `W-L10N-MISSING`, promotable with `--deny`. A malformed bundle is
  `E-LOCALE-BUNDLE`.
- **`lute compile --all --project <dir> -o <dir>`** — project-wide compile
  emitting one artifact per document plus `project.index.json`, whose
  `entities`/`enums`/`relations`/`seedFacts`/`rules`/`prereqEdges` are the
  deterministic **union** across every document. The runtime contract already
  required engines to compute that union; until now every adopter re-implemented
  it. All-or-nothing: one failing document writes no output.
- **`identity:` templates** — `lute.project.yaml` can now shape `lineId` and
  `voiceKey` (`{prefix}`, `{speaker}`, `{code}`), so a catalog with an existing
  identity convention can be migrated. Defaults reproduce 0.7.0 byte-for-byte;
  an unknown token is `E-IDENTITY-TEMPLATE`.
- **Plugin `stampAttrs`** — a plugin may declare **cross-cutting** attributes
  admissible on every directive *and* on content lines, landing flattened in the
  record's stamp. Engines routinely carry per-record metadata orthogonal to the
  record kind (analytics tags, bonus hooks); 0.0.1 could declare attributes only
  per-directive. `stampAttrs` participates in `capabilityVersion`.
- **Declarative lowering is implemented** — `lower: { record, fields }` parsed
  since 0.0.1 but `lute-compile` never matched on it, so *every* plugin
  directive became `kind: "plugin"`. A directive may now lower to one of the
  eight non-control-flow staging kinds, with `fromAttr`/literal field bindings
  validated at assembly (`E-LOWER-RECORD-UNKNOWN`, `E-LOWER-RECORD-FIELD`). The
  emitted record inherits the target kind's `wait` default, so it is
  indistinguishable from the core directive an engine dispatches identically.
- **Browser playground** — the website ships a fully client-side
  [Try Lute](https://lute-lang.vercel.app/playground/) page: a new `lute-wasm`
  crate compiles the checker, compiler, and tracer to WebAssembly (2.3 MB,
  committed at `packages/website/public/playground/pkg/` so the site build stays
  Rust-free), exposing `check_source` / `compile_source` / `trace_source` /
  `version`. Live diagnostics with click-to-seek, an on-demand compiled-IR view,
  a mock-driven trace transcript, and three embedded checker-clean examples.
  Scope: one self-contained document, core profile (no `uses:` or plugins).
- **LSP stale-binary version guard** — `lute-lsp` advertises the language
  version it implements (`lute_check::LUTE_LANG_VERSION`) as the LSP
  `serverInfo.version`, and the VS Code extension warns once when the running
  server is strictly older than a document's frontmatter `luteVersion:` target.
  A stale server silently mis-analyzes newer grammar and cannot self-detect it
  (its own `W-LUTE-VERSION-STALE` compares against the version it was built at),
  so the client-side comparison is the only reliable signal. Toggle with
  `lute.versionCheck` (default on). Diagnostics remain a byte-for-byte
  reprojection of the CLI (`crates/lute-lsp/tests/divergence.rs`).

### Changed

- **Author `state:` is scalar — enforced** (`E-STATE-COLLECTION`). Three sources
  disagreed: the normative text said scalar-only, the shape validator accepted
  the full `Type` union (so `type: { list: string }` silently passed), and
  `docs/runtime/state-lifecycle.md` documented `list<…>`/`map<…>`/`record` as
  valid. All three now agree — collection-shaped `StateEntry` types reach the
  artifact only through a plugin `state_shapes` expansion. **This is the one
  case where a 0.7.0-clean document may newly fail**; collections were always
  meant to be modelled as `relations:` (0.3.0 §3).
- **`E-`-severity capability-resolution diagnostics now gate the exit code.**
  Project/plugin resolution errors print on the `lute:` channel rather than the
  per-document diagnostic list, and were previously advisory — so a new
  `E-PLUGIN-OPTION-TYPE` would have printed and passed. They now fail
  `check`/`check-project`/`compile`/`test`, matching the binary-severity rule
  (`E-` gates). The forced-single-root reconciliation scan behind
  `compile --project` is exempt: a sibling belonging to a nested subproject
  legitimately mis-resolves under a forced root, and that is not the target
  document's fault.
  Because that text is now the whole of what a failing author sees, every
  `AssembleError` renders prose: the five variants that previously fell back to
  a Rust `Debug` form (`E-PLUGIN-MISSING-ACTIVE`, `E-PLUGIN-DUP-ACROSS` /
  `E-DOMAIN-DUP`, `E-PLUGIN-RESERVED-NAME`, `E-STATE-SHAPE-CYCLE`,
  `E-PLUGIN-UNKNOWN-ASSETKIND`) now say what went wrong and what to do. Codes
  are unchanged.
- **Plugin option validation** (spec Appendix C1) — activation rejects an
  unknown option name (`E-PLUGIN-OPTION-UNKNOWN`) and a value that fails its
  declared type (`E-PLUGIN-OPTION-TYPE`).
- **Plugin frontmatter value validation** (C2) — a plugin-owned frontmatter key
  is now checked against its declared schema, not merely admitted
  (`E-FRONTMATTER-SCHEMA`).
- **Reserved stamp-attribute names** (C4, widened) — a plugin declaring `at`,
  `duration`, `delay`, `wait`, `timeline`, `provenance`, or `source` as an
  attribute is rejected at assembly with `E-PLUGIN-RESERVED-STAMP-ATTR`, on both
  the `stampAttrs` and per-directive surfaces.
- **`lute init` / `lute new` stamp the current language version** instead of a
  hardcoded literal, so scaffolds cannot go stale on a version bump again.

### Not done, deliberately

Recorded so they are not re-proposed — each was rejected on measured evidence,
see [`scenario-dsl/0.8.0.md`](docs/proposals/scenario-dsl/0.8.0.md) §10:
Datalog aggregation (`sum`), any Datalog surface extension, author-declarable
collection state, and a per-record `label` field. Plugin spec item **C3**
(the `wait="false"` stale-default bridge read) remains open: it needs a
dominance analysis the checker does not perform, and is deferred rather than
half-shipped.

## [0.7.0] - 2026-07-20

### Changed

- **Version unification** — every version axis is aligned at `0.7.0`. The
  language (`LUTE_LANG_VERSION`, was `0.6.1`), the IR (`LUTE_IR_VERSION`, was
  `0.6.1`), the Cargo workspace toolchain (was `0.2.0`), and all four npm
  packages (were `0.2.0`) now share one visible number. This supersedes the
  `0.2.0` toolchain release below, which shipped the same day as the last
  independently-numbered toolchain: `0.7.0` is the unified number for that work
  plus the additions here. There is **no grammar, semantic, or IR shape change**
  — language `0.7.0` is byte-for-byte `0.6.1` semantics (see
  [`docs/proposals/scenario-dsl/0.7.0.md`](docs/proposals/scenario-dsl/0.7.0.md)).
  The IR JSON schema is renamed `schemas/lute-ir-0.6.schema.json` →
  [`schemas/lute-ir-0.7.schema.json`](schemas/lute-ir-0.7.schema.json) (body
  unchanged). A document stamped `luteVersion: "0.6.1"` now fires
  `W-LUTE-VERSION-STALE`; the remedy is to restamp it `luteVersion: "0.7.0"`.

### Added

- **`lute run` reference runner** — an executable reference interpreter for
  compiled artifacts, validated against the `conformance/` fixture corpus so an
  engine has a golden oracle for artifact execution semantics.
- **`lute test` scenario tests + coverage** — a scenario test runner with
  coverage reporting over authored paths, so authors can assert reachable
  outcomes and see which regions a suite exercises.
- **`lute init` / `lute new` / `lute doctor`** — project scaffolding
  (`init`/`new`) and an environment/health diagnostic (`doctor`).
- **`lute scenario --format json|dot`** — machine-readable (`json`) and
  Graphviz (`dot`) exports of the scenario graph alongside the human view.
- **`lute loc` export/report** — localization string export and a coverage
  report over translatable content.
- **New website pages** — `getting-started/learning-paths`, a tutorial track,
  a "when to use" fit page, and the `spec/current` consolidated spec index.
- **Docs CI** — a continuous-integration workflow that runs the docs
  consistency checker and builds the website on every change.
- **VS Code extension packaging** — the editor extension is packaged and a
  `.vsix` artifact is produced as a CI build output.

## [0.2.0] - 2026-07-20

### Added

- **Runtime contract documentation** — a runtime docs set under
  [`docs/runtime/`](docs/runtime/) plus a website page at `tooling/runtime-contract`
  describing what a compiled artifact promises an engine, and the honest
  boundaries of static analysis (reachability is conservative under declared
  `after:` routes; relational gates can yield `Unknown` verdicts requiring
  human review; `lute trace` walks one deterministic mock-driven path, not a
  proof over all paths).
- **Versioned IR JSON schema** — [`schemas/lute-ir-0.6.schema.json`](schemas/lute-ir-0.6.schema.json),
  a machine-readable schema for the compiled artifact envelope, letting engines
  validate artifacts against the `irVersion` they stamp.
- **`lute version`** — prints the toolchain, language, and IR versions;
  `lute version --json` emits `{"toolchain":…,"language":…,"ir":…}` for tooling.
- **Windows x86-64 prebuilt binaries** — the npm launcher now resolves a
  native binary on `win32-x64` in addition to `darwin-arm64` and `linux-x64`.
- **Investigation RPG example** — a worked example exercising quests,
  objectives, relational state, and connectivity analysis.
- **`LICENSE`** — the project is MIT-licensed.
- **`docs/versioning.md`** — the versioning policy: the toolchain / language /
  IR / capability / plugin axes, which bumps when, and the pre-1.0 draft
  breaking-change policy.

### Changed

- **Homepage repositioning** — the README and website landing now split the
  status claim along its axes (language draft vs. implementation shipped vs.
  production stability) rather than a single blanket "implemented" claim, and
  link `LICENSE`, this changelog, and the versioning policy.

## [0.1.0]

Initial scoped npm release: the [`@lute-lang/lute`](https://www.npmjs.com/package/@lute-lang/lute)
launcher resolving `darwin-arm64` and `linux-x64` prebuilt binaries, targeting
language version `0.6.1`.

[0.13.0]: https://github.com/journeyWorker/lute/releases/tag/v0.13.0
[0.12.0]: https://github.com/journeyWorker/lute/releases/tag/v0.12.0
[0.11.1]: https://github.com/journeyWorker/lute/releases/tag/v0.11.1
[0.7.0]: https://github.com/journeyWorker/lute/releases/tag/v0.7.0
[0.2.0]: https://github.com/journeyWorker/lute/releases/tag/v0.2.0
[0.1.0]: https://github.com/journeyWorker/lute/releases/tag/v0.1.0
