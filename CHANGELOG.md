# Changelog

All notable changes to the Lute **toolchain** are documented here. The format
is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Lute tracks three independent version axes; this file covers only the first:

- **Toolchain** — this changelog. The version of the CLI, checker, compiler,
  LSP, and npm launcher that ship together, stamped from the Cargo workspace
  (`CARGO_PKG_VERSION`) and printed by `lute version`.
- **Language** — currently `0.21.0`, the grammar and semantics the checker
  enforces. Its history lives in the versioned spec stack under
  [`docs/proposals/scenario-dsl/`](docs/proposals/scenario-dsl), not here.
- **IR** — the compiled JSON artifact schema, stamped as `irVersion` in every
  artifact (currently `0.21.0`) and gated on by consuming engines.


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
