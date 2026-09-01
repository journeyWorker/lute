# Changelog

All notable changes to the Lute **toolchain** are documented here. The format
is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Lute tracks three independent version axes; this file covers only the first:

- **Toolchain** — this changelog. The version of the CLI, checker, compiler,
  LSP, and npm launcher that ship together, stamped from the Cargo workspace
  (`CARGO_PKG_VERSION`) and printed by `lute version`.
- **Language** — currently `0.12.0`, the grammar and semantics the checker
  enforces. Its history lives in the versioned spec stack under
  [`docs/proposals/scenario-dsl/`](docs/proposals/scenario-dsl/), not here.
- **IR** — the compiled JSON artifact schema, stamped as `irVersion` in every
  artifact (currently `0.12.0`) and gated on by consuming engines.

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
assessing Lute against a real, large game catalog — 777 authored scenes /
73,847 command rows / 583 quests / 3,104 condition rows
([`docs/adoption/oshiz-assessment.md`](docs/adoption/oshiz-assessment.md) §10).
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
