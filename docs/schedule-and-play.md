# Schedule & play

`schedule.yaml` places a project's scenes on a tick clock instead of leaving
reading order to file position, and `lute play <PROJECT_DIR>` walks that
schedule into one chained, reviewer-facing transcript — the order a player
following one route actually sees. Both are new in **0.11.0**; both are
**toolchain-only**: `schedule.yaml` carries no `kind:`/`luteVersion:`, no
project `uses:` resolution, no capability fold, and neither the language nor
the IR moved to add them. Nothing in `lute-check`/`lute-compile` has ever
heard of `schedule.yaml` — it is a CLI-owned project file, read only by
`lute-cli`'s own `schedule.rs`/`play.rs`, sitting beside `lute.project.yaml`.

This page is the full key and diagnostic reference. The design rationale —
why the language doesn't grow an ordering primitive, why `lute play` requires
a schedule with no `after:`-graph fallback, and the eighteen review findings
that shaped the two-phase cursor and the world-lane drain rule — lives in the
design spec:
[`docs/superpowers/specs/2026-08-14-lute-schedule-and-play-design.md`](superpowers/specs/2026-08-14-lute-schedule-and-play-design.md).
The driving consumer is OSHiZ onboarding
(`packages/content-catalog/scratchpads/main-story/onboarding/lute/` in the
eevee monorepo) — every example below is real output from that project.

**Why a schedule at all.** A project's sibling route files are deliberately
unguarded — file split *is* the route — so a topological walk over `after:`
edges cannot select one route through them; it would play every sibling.
`lute play` therefore refuses a project with no `schedule.yaml` outright
(exit 2), naming this spec, rather than falling back to some other ordering.

## `schedule.yaml`

```yaml
clock:
  buckets: [dawn, morning, late_morning, afternoon, late_afternoon, evening, night, midnight]
  ticksPerBucket: 12
  days: 7

lanes:
  user: { exclusive: true, idleThreshold: 0 }
  world: { exclusive: false }

assume:
  - "run.inflow != 'none'"

placements:
  - event: kuhen
    lane: user
    at: d2.morning+0
    size: 4
    variants:
      - when: "run.inflow == 'iroha'"
        doc: scenes/kuhen/iroha.lute
      - when: "run.inflow == 'reiha'"
        doc: scenes/kuhen/reiha.lute
```

### `clock:`

| Key | Type | Meaning |
|---|---|---|
| `buckets` | list of names | Named time-of-day buckets, in order (e.g. `[dawn, morning, …, midnight]`). Non-empty, no duplicates. |
| `ticksPerBucket` | positive int | Ticks per bucket. The story clock is `buckets.length × ticksPerBucket × days` ticks, checked for `u32` overflow. |
| `days` | positive int, default `1` | Number of days. Days are **1-based** — `d0` is rejected. |

### `lanes:`

A map of lane name → `{ exclusive: bool, idleThreshold?: int }`. Only two
conventional names are used by the driving project (`user`, `world`), but any
name is legal — a placement's `lane:` just has to match one declared here.

- `exclusive: true` — co-satisfiable placements on this lane may not have
  overlapping `[at, at+size)` intervals (`E-SCHED-USER-OVERLAP`). Use this for
  a single-threaded player-facing lane.
- `exclusive: false` — overlap is allowed by design. Use this for a
  background/world lane whose events do not wait for the player.
- `idleThreshold` — overrides `W-SCHED-IDLE`'s 24-tick default pacing-gap
  threshold for this lane; `0` disables the check entirely (a sparse
  multi-day lane, like the onboarding project's 672-tick `user` lane, is a
  choice, not a smell — its `schedule.yaml` sets `idleThreshold: 0`).

### `assume:`

A list of CEL guard strings (same surface as `when:`, below) declaring an
upstream contract the schedule can rely on. The static route-space sweep
(`E-SCHED-VARIANT-GAP`/`-AMBIG`, `E-SCHED-USER-OVERLAP`) enumerates every
combination of enum-typed scalar domains referenced by guards, and an
assumption that evaluates **definitively false** for a given assignment prunes
it from the sweep entirely — it is never checked, never reported. An
assumption that evaluates `Unknown` (a non-enum scalar, or a malformed guard)
keeps every assignment: an assumption is only ever a way to *narrow* the
sweep, never to hide a real gap. The onboarding project uses this to prune
`run.inflow == 'none'`, the pre-assignment sentinel that never reaches a
placed scene, from `E-SCHED-VARIANT-GAP` findings on placements that only
cover the six real inflow channels.

### `placements:`

Each entry is one **event** occupying a `[at, at+size)` interval on one
**lane**. Two mutually exclusive forms:

- **Single-doc form** — `doc: <path>` directly on the placement. Always
  satisfiable, no guard. For a world event with exactly one unguarded variant
  (the design spec's worked `nera-recon` example).
- **Guarded form** — `variants:` (non-empty list), each a
  `{ when, doc, at?, size?, presentation? }`. At most one variant may be
  satisfiable for a given route's state; which one runs is resolved live, at
  play time, not statically baked in.

Giving neither `doc:` nor `variants:`, giving **both**, or giving an empty
`variants:` list is `E-SCHED-VARIANT-FORM`.

| Key | Where | Type | Meaning |
|---|---|---|---|
| `event` | placement | string | Event identity — matches the project's `scenes/<event>/` directory convention (§ naming below). The `(event, lane)` pair must be unique (`E-SCHED-EVENT-DUP`); the same event MAY appear on two different lanes. |
| `lane` | placement | string | Must name an entry under `lanes:` (`E-SCHED-LANE-UNKNOWN`). |
| `at` | placement, variant | `[dN.]bucket+tick` or absolute int | Interval start. See **`at:` grammar** below. Omissible on a placement — see **cursor resolution**. A variant's `at` is unset by default (inherits the placement's own resolved `at`) unless explicitly overridden. |
| `size` | placement, variant | int ≥ 1 | Interval length in ticks; the interval is half-open `[at, at+size)`. `0` is `E-SCHED-SIZE-INVALID`, checked on the placement AND independently on every variant that overrides it. |
| `presentation` | placement, variant | int, default `100` | Execution-order key (lower plays first). See **execution-order contract** below. A variant may override its placement's `presentation`, same as `at`/`size`. |
| `optional` | placement | bool, default `false` | `true` legalizes zero satisfiable variants for some route assignment — `E-SCHED-VARIANT-GAP` is suppressed for this placement. Use for content that only some lanes/routes have written yet. |
| `variants[].when` | variant | CEL string or absent | Guard read against the **content-line CEL surface** — state comparisons plus `holds()`/`count()` — never the `after:` prerequisite profile. Absent only on the single-variant unguarded shorthand (always satisfiable). A malformed guard is `E-SCHED-GUARD-PARSE` (not in the design spec's original table — added because a broken guard has to be reported somewhere, not silently folded to always-unknown). |
| `variants[].doc` | variant | project-relative path | The `.lute` document this variant plays. Must exist under the project root (`E-SCHED-DOC-MISSING`); an absolute path or a `..`-escaping path is rejected *before* the filesystem is even touched (`E-SCHED-DOC-PATH`) — an existing file outside the project can never pass by accident. |

A scene doc that exists under the project but is referenced by no placement
variant is `W-SCHED-DOC-UNPLACED` (component fragments excluded — they have
no identity of their own, mirroring `compile --all`).

### Same event, different position per route

A variant's `at`/`size`/`presentation` override is how "the same event sits
at a different position on a different route" is expressed — never a
differently-numbered file. From the onboarding schedule:

```yaml
- event: errand-open
  lane: user
  optional: true
  at: d2.late_afternoon+0
  size: 4
  variants:
    - when: "run.inflow == 'ann'"
      doc: scenes/errand-open/ann.lute
      # ann/reiha keep the placement's own at/size (daytime)
    - when: "run.inflow == 'megumi'"
      doc: scenes/errand-open/megumi.lute
      at: d2.evening+0          # this scene is fixed-evening in-fiction
    - when: "run.inflow == 'iroha'"
      doc: scenes/errand-open/iroha.lute
      at: d2.night+0
      size: 4
```

### `at:` grammar

Two shapes, both resolved to an absolute tick against `clock:`:

- **Symbolic** — `[dN.]<bucket>+<tick>`. `dN.` is optional and defaults to
  `d1` (`morning+2` == `d1.morning+2`). `N` is 1-based; `d0` is rejected.
  `<bucket>` must name a declared bucket. `<tick>` must satisfy
  `0 ≤ tick < ticksPerBucket`. Any other shape (missing `+`, non-numeric day
  or tick, unknown bucket, out-of-range tick) is `E-SCHED-AT-PARSE`.
- **Absolute** — a bare non-negative integer tick count from clock start
  (e.g. `at: 108`). A negative integer is `E-SCHED-AT-PARSE`.

A resolved `[at, at+size)` interval that exceeds the story clock, or whose
own arithmetic overflows `u32`, is `E-SCHED-CLOCK-OVERFLOW` — deliberately a
*different* code from a malformed shape, so an overflowing schedule is never
misread as a typo (a well-formed coordinate that resolves past the clock
diagnoses overflow; a badly-shaped one diagnoses a parse error, never both
under one code).

### Cursor resolution (two-phase, static)

**Phase 1 — declaration order.** Each user-lane placement's base `at`
resolves in the order it appears in `placements:`. An **omitted** `at`
inherits the previous placement *on the same lane*'s resolved `at + size`
(never route-independent, since it's the placement's own base fields, not a
variant override). Omitting `at` right after a declaration predecessor whose
own variants override `at`/`size` per route is `E-SCHED-CURSOR-DYNAMIC` — a
dynamic cursor cannot be statically resolved, and every later omitted-`at`
placement on that lane stays unresolved too (no cascade of duplicate
diagnostics — the root cause is reported exactly once).

**Phase 2 — presentation sort.** Execution order is
`(presentation, resolved at, declaration index)` — seen next.

### Static checks, run order

`schedule.rs`'s `static_check` covers clock structure, `at:`/size shape and
overflow, doc existence/path safety, and pacing; `route_space_check` (needing
CEL evaluation over the full enum-domain cross-product, capped at **4096**
assignments above which the whole sweep is skipped with
`W-SCHED-ROUTESPACE-CAP` rather than silently truncated) covers guard parse,
variant gap/ambiguity, and same-lane overlap. Both run before `lute play`
compiles anything; any Error-severity finding halts the walk at exit 1
without touching a single `.lute` document.

## Naming convention (project-side, not enforced by the checker)

```
scenes/<event>/<variant>.lute      # scenes/confinement/iroha.lute
routes/<route>[-<label>].play.yaml
schedule.yaml
```

Canonical node identity is a scene doc's authored `id: <event>-<variant>`
(dsl 0.15.0 §2), so `scenes/confinement/iroha.lute` writes
`id: confinement-iroha` and simply omits `season:`/`episode:` — the frozen-
numbers workaround (`character: <event>-<variant>` plus opaque season/episode
integers) that predated authored ids is no longer needed, and existing scenes
can keep their derived key or migrate at their own pace. What retires is the
*rule* that a number encodes reading order: position now lives only in
`schedule.yaml`, so inserting a scene is a schedule edit, never a renumbering.

## `lute play`

```
lute play <PROJECT_DIR>
  --state run.inflow=iroha ...      # seed scalars (route selection)
  --fact  "..." ...                 # seed facts
  --script routes/iroha.play.yaml   # route script (choices; may also carry seeds)
  --choose kuhen/coffeeOrder=recommend ...  # ad-hoc override, event-qualified
  --auto first                      # unattended policy for unscripted decisions
  --lanes user|all                  # default: user (strict player view)
  --steps N                         # stop after N presented placements
  --coverage <FILE>...              # review-gap corpus replay (repeatable)
  --json
```

Compiles the WHOLE gated project once in memory — scene *and* quest kind,
the same declaration union `compile --all` writes (relations, rules, seed
facts, state tables, prereq edges, and quest/`<on>` declarations union across
every document, including quest docs, which are never placed) — then walks
the schedule's user-lane placements in presentation order, re-evaluating each
event's guarded variants against **live** state and threading `run.*`/
`user.*`/`app.*`/`quest.*` state and facts across scene boundaries through
`lute run`'s own reference evaluator. `scene.*` always resets to the entering
scene's own declared defaults at every scene boundary.

### Route selection

- `--state <PATH=LITERAL>` (repeatable) — a scalar state seed.
- `--fact "<REL(ARG…)>"` (repeatable) — a ground fact.
- `--script <FILE>` — a route script (below), this module's **own** closed
  grammar. It is **not** a `lute trace --mock` file: v1 of this design
  claimed that reuse and review called it an error, because a route script's
  `choose:` keys are event-qualified (`<event>/<hubOrBranchId>`), a shape the
  mock family's grammar has no notion of.
- `--choose <EVENT/ID=CHOICEID[,CHOICEID…]>` (repeatable) — an ad-hoc
  decision, event-qualified. A bare (unqualified) id is legal only when
  unique across the whole schedule; a colliding bare id is a script error
  naming every event it collides in.
- `--auto first` — picks the first eligible option for anything left
  unscripted, at **every** hub re-presentation, not just the first pass.

CLI flags win over a route script's `state:`/`choose:` on a same-key
conflict; `facts:` union (CLI seeds never displace a scripted fact, they add
to it).

### Route scripts (`*.play.yaml`)

```yaml
state:
  run.inflow: iroha
choose:
  kuhen/coffeeOrder: [recommend]         # event-qualified hub/choice id
  reassure: [calmly]                     # bare id — legal only if unique across the schedule
  # a real <hub>: forced picks in visit order, one entry per re-presentation
  deduction/victoria: [askAzuki, rereadRecord, lookDesk]
```

The only legal top-level keys are `state:`, `facts:`, `choose:` — anything
else is a script error naming the offending key. `choose:` values are a
choice id or a list of choice ids (for a hub visited more than once, or
picked through more than once per visit); selections are consumed in visit
order.

### Execution-order contract

Variant selection and tick resolution produce a **presentation-ordered
execution sequence** — `(presentation, resolved at, declaration index)`.
Story ticks are retained for labels and world synchronization only; tick
order is a tie-break within equal presentation keys, never the primary key.
State accumulates in presentation order — the order the player experiences
it — so a cold-open flashback declared `presentation: 0` can legitimately run
first even though its story tick is chronologically last (the onboarding
project's `confinement` placement: story tick `d6.midnight+0`, near the end
of the clock, `presentation: 0`, plays first).

### Chained evaluation — the boundary loop

Before each placement (in presentation order): re-evaluate its variants'
guards against current state.

- Exactly one satisfiable → play it.
- Zero on a non-`optional` event → `E-SCHED-VARIANT-GAP`, halt exit 1.
- Two or more → `E-SCHED-VARIANT-AMBIG`, halt exit 1.
- Zero on `optional: true` → skip, no halt.

Before a scene runs, its `after:` prerequisite is evaluated against the
visited/completed sets accumulated **in presentation order** (never file
order) — a violation is `E-SCHED-AFTER-ORDER`, halt exit 1. This is why the
cold-open-first structure works: day-1 scenes declare
`after: visited(confinement…)`, and `confinement` has already presented by
the time they're checked, even though its story tick is later.

`::end` terminates the whole playthrough, its reason surfaced in the
transcript's end line.

**Quest lifecycle is a scoping decision, not a gap that fails silently.**
Quest-declared state (`quest.<id>.state`) is in the project-wide state union,
so a guard referencing it type-checks — but `lute play` deliberately does
**not** drive a quest artifact's own `<quest>`/`<on>` lifecycle as part of
the chain (the reference `Runner::run_quest` does one fresh fixpoint settle
per call; invoking it repeatedly across scene boundaries would silently
re-fire an already-complete objective's body). Consequence, honestly
surfaced: `completed(...)`/`active(...)` inside an `after:` causality check
always evaluate against an **empty set** for the whole playthrough — a
placement causally gated on quest completion halts loudly as
`E-SCHED-AFTER-ORDER` (**exit 1**, the ordinary causality-violation tier,
never a silent "always true" or a separate incomplete tier). This is
confirmed harmless for the driving onboarding project: nothing in it gates
`after:` on `completed`/`active`, and it ships no quest docs yet.

### World lane and rewind

Scenes are atomic schedule units — there is no intra-scene tick mapping.

- **Drain rule.** After a user placement completes at story end-tick `T`,
  every world placement whose start tick lies within the current
  **segment**'s covered story range and has not yet fired executes once
  each, atomically, in `(at, declaration index)` order — *before* the next
  user placement's guards are evaluated. `--lanes user` still **executes**
  them (state must not depend on rendering); it only omits them from the
  transcript.
- **Segments and rewind.** Consecutive user placements with non-decreasing
  story ticks form a segment; a presentation jump backward starts a new
  segment and is **purely cinematic** — no state rolls back, nothing
  replays, the world cursor restarts at the new segment's start tick. Each
  world placement fires at most once per whole playthrough. A world
  placement covered only by a segment that plays "in the future" of a
  later-presented segment is `W-SCHED-WORLD-IN-FLASHBACK` (its effects would
  precede, in experienced order, story time it belongs after — a design
  smell, not a halt).

The onboarding project's `world` lane is currently empty by design (reserved
for background/NPC events independent of the player's route); the mechanics
above are exercised by the design spec's own worked `nera-recon` example and
by `crates/lute-cli/tests/play.rs`'s rewind/drain test suite.

### Transcript

Human format, real output (`--lanes all`, iroha route):

```
── d6.midnight+0 (tick 564) · user · confinement/iroha ──────────────
::background{location="local_mart_indoor" time="midnight" wait=true}
@iroha{emotion="anxious" voiceKey="iroha-0010"}: 안 열려요!!! 어떡해요!!!!
▷ choice calmDown: [breathe] sortOut        ← chosen: breathe
...
⏪ d6.midnight+6 → d1.late_afternoon+0 (rewind, tick 570 → 48)
── d1.late_afternoon+0 (tick 48) · user · arrival/trunk ──────────────
...
⏩ tick 52 → 60 (fast-forward, empty user lane)
── d1.evening+0 (tick 60) · user · office-scene/trunk ──────────────
...
▷ hub victoria: [askAzuki] rereadRecord lookDesk        ← chosen: askAzuki
▷ hub victoria: askAzuki [rereadRecord] lookDesk        ← chosen: rereadRecord
...
── end: clock exhausted (tick 672) ──────────────────────────
```

- **`── <tickLabel> (tick <N>) · <lane> · <event>/<variant> ──`** — a
  placement header. `<variant>` is the played variant's `doc` file stem.
- **`⏩ tick A → B (fast-forward, empty user lane)`** — the story clock
  advanced across ticks with nothing scheduled.
- **`⏪ <from> → <to> (rewind, tick A → B)`** — a presentation jump
  backward starts a new segment (see world lane above); purely cinematic.
- **`▷ choice <id>: opt1 [opt2] opt3        ← chosen: opt2`** — every
  offered option, the chosen one bracketed, restated after the arrow. A
  hub's own re-presentation prints one line per pick, options narrowing as
  each `once` arm retires.
- **`▷ choice <id>: opt1 opt2        ← INCOMPLETE (no decision)`** — an
  unscripted decision with no `--auto` policy set; the halt line right
  after names the event, doc, decision kind, id, and every still-eligible
  option.
- **`⚠ W-SCHED-TIME-MISMATCH: ::bg time="<t>" vs schedule bucket "<b>"`** —
  the scene's *first* `::bg` directive's `time=` disagrees with the
  placement's start bucket. `time="day"` is treated as matching any of
  `morning`/`late_morning`/`afternoon`/`late_afternoon`; only the first
  `::bg` in a scene is checked (no command-to-tick mapping exists to check
  the rest).
- **`⚠ W-SCHED-WORLD-IN-FLASHBACK: '<event>' drains during a rewound
  segment`** — see world lane above.
- **`── end: <reason> ──`** — `clock exhausted (tick N)`, `::end` reached,
  or `stopped after N step(s)` under `--steps`.
- **`── halted: [<CODE>] <message> ──`** — a runtime halt (see exit codes).

`--json` emits `{"exit", "endReason", "scenes": [...]}`. Each scene record
carries `event`, `variant`, `doc`, `lane`, `tick`, `tickLabel`, `endTick`,
`stateDelta`, `fastForwardFrom`, `rewindFrom`, `timeMismatch`,
`worldInFlashback`, and `commands` — the same command-record shapes
`lute run --json` emits (`line`, `choice`/`hub` with `branch`/`hub`, `chose`,
`assert`, `background`, `sfx`, `music`, `vfx`, …), addressed by `addr`. A
`choice`/`hub` record carries only the id chosen (`chose`), not the full
offered-options list — the artifact's own `commands[].options` is where the
full option set lives, which is what `--coverage` reads to compute an
uncovered-option report (below). Deterministic: the same seeds and script
produce byte-identical output, JSON or human.

### Coverage (`--coverage`)

```
lute play <PROJECT_DIR> --coverage routes/a.play.yaml --coverage routes/b.play.yaml …
```

Replays every named route script through the same chain executor,
per-script transcript rendering suppressed, and reports every placement,
variant, and hub/choice option the **corpus as a whole** never exercises —
the review-gap detector. `--coverage <FILE>` is repeatable and takes exactly
**one file per flag** — there is no glob expansion inside the flag itself.
Passing a shell glob directly (`--coverage routes/*.play.yaml`) fails with a
clap usage error, because the shell expands it into several bare positional
arguments the command doesn't accept:

```
$ lute play . --coverage routes/*.play.yaml
error: unexpected argument 'routes/ann.play.yaml' found
```

Repeat the flag instead — shell-expand the glob yourself into one
`--coverage` per file:

```sh
lute play . $(for f in routes/*.play.yaml; do printf -- '--coverage %s ' "$f"; done)
```

Real output, full coverage (the onboarding project's twelve route scripts):

```
lute play coverage: 12 script(s) replayed
  ✓ routes/ann.play.yaml: clock exhausted (tick 672) (5 scene(s))
  ✓ routes/iroha.play.yaml: clock exhausted (tick 672) (17 scene(s))
  ...
placements: 18/18 presented
variants: 32/32 selected
hub/choice options: 114/114 chosen
── COVERED ──
```

`--coverage` is **exclusive** with `--script`/`--choose`/`--steps` — a single
playthrough's own knobs don't compose with a corpus replay (which script's
choices? which step limit, applied to every corpus member?). `--json` emits
`{"exit", "scripts": [...], "placements": {total, presented, uncovered},
"variants": {...}, "options": {...}}`.

## Exit codes

| Code | Plain `lute play` | `--coverage` |
|---|---|---|
| `0` | Complete: `::end` reached, or the clock ran out. | Full coverage — the corpus exercised every placement, variant, and hub/choice option. |
| `1` | A halted walk named by its `E-SCHED-*` code: `VARIANT-GAP`, `VARIANT-AMBIG`, `AFTER-ORDER` — or a static `schedule.yaml`/route-space Error-severity diagnostic caught before any document compiles. | A coverage gap remains — at least one placement/variant/option was never exercised across every script. |
| `2` | A usage or I/O failure: no `schedule.yaml` at all, a malformed route script, a vocabulary conflict across the project's documents, `--coverage` combined with `--script`/`--choose`/`--steps`, or a clap argument error. | Same. |
| `3` | **Incomplete**: an unscripted choice/hub decision with no `--auto` policy, or a guard/effect the reference runner genuinely cannot resolve (below). | At least one corpus script itself halted incomplete before finishing — its coverage contribution is partial. |

## Unsupported surfaces

`lute play` is a **reference-runtime preview**, not a player-session
emulation. Three surfaces it cannot resolve, each handled honestly rather
than silently:

- **`now()`/`validAt(...)`** in a guard or effect — no reference-runtime
  resolution exists for wall-clock time. Halts **incomplete, exit 3**,
  naming the event and doc.
- **An unresolved plugin `bridgeResult` effect** — no bridge was invoked to
  produce it. Halts **incomplete, exit 3**.
- **Quest chains** (`<quest>`/`<on>` lifecycle driven across scene
  boundaries) — deliberately out of scope, not merely unresolved: see
  *chained evaluation* above. A placement's `after:` gated on
  `completed(...)`/`active(...)` always halts **`E-SCHED-AFTER-ORDER`, exit
  1** — the ordinary causality-violation tier, since the empty
  completed/active set is a defined (if pessimistic) answer, not an unknown.

Wall-clock `<timeline>` pacing is **never** simulated and never gates a halt
on its own — it has no observable effect on state or control flow to be
honest about (a `barrier` record in the underlying `lute run` transcript is
a note only, mirrored here).

## Diagnostic reference

Fifteen static errors (schedule-shape and route-space), one runtime error,
and five warnings — the full `E-SCHED-*`/`W-SCHED-*` set, collected from the
`schedule.rs`/`play.rs` constants that define them.

### Errors — static (`schedule.rs`, reported before any document compiles)

| Code | Fires when |
|---|---|
| `E-SCHED-CLOCK-STRUCTURE` | `clock.ticksPerBucket` or `clock.days` is `0`, or `clock.buckets` is empty. |
| `E-SCHED-BUCKET-DUP` | `clock.buckets` names the same bucket twice. |
| `E-SCHED-LANE-UNKNOWN` | A placement's `lane:` names no entry under `lanes:`. |
| `E-SCHED-EVENT-DUP` | The same `(event, lane)` pair is placed more than once. |
| `E-SCHED-VARIANT-FORM` | A placement gives neither `doc:` nor `variants:`, gives both, or gives an empty `variants:` list. |
| `E-SCHED-SIZE-INVALID` | A resolved `size` (placement or a variant that overrides it) is `0`. |
| `E-SCHED-AT-PARSE` | A raw `at:` is malformed: not `[dN.]bucket+tick` or an integer, `d0`, an unknown bucket, or a tick offset outside `[0, ticksPerBucket)`. |
| `E-SCHED-CURSOR-DYNAMIC` | An omitted `at:` immediately follows a same-lane declaration predecessor whose variants override `at`/`size` — a dynamic cursor cannot be statically resolved. |
| `E-SCHED-CLOCK-OVERFLOW` | A resolved `[at, at+size)` interval exceeds the story clock, or its own arithmetic overflows `u32`. |
| `E-SCHED-DOC-MISSING` | A variant's `doc:` is a legitimately project-relative path, but no file exists there. |
| `E-SCHED-DOC-PATH` | A variant's `doc:` is authored as an absolute path, or escapes the project root via `..` — checked before the filesystem is touched, so an existing file outside the project can never pass by accident. |
| `E-SCHED-VARIANT-GAP` | A non-`optional` placement has no satisfiable variant for some enum-domain route assignment. |
| `E-SCHED-VARIANT-AMBIG` | Two variants of the same placement are co-satisfiable for some route assignment. |
| `E-SCHED-USER-OVERLAP` | Two co-satisfiable placements on the same **exclusive** lane have overlapping `[at, at+size)` intervals (a possibly-co-satisfiable pair whose guard references a non-enum scalar demotes this to a **warning** instead — it is Unknown, not proven). |
| `E-SCHED-GUARD-PARSE` | A `when:` guard's or an `assume:` entry's CEL text fails to parse. |

### Error — runtime (`play.rs`, during the walk)

| Code | Fires when |
|---|---|
| `E-SCHED-AFTER-ORDER` | A scene's `after:` prerequisite is unsatisfied against the visited/completed sets accumulated in **presentation** order (or, per the quest-lifecycle scoping decision above, gated on quest completion at all). |

### Warnings

| Code | Fires when |
|---|---|
| `W-SCHED-DOC-UNPLACED` | A project scene doc exists but no placement variant references it (component fragments excluded). |
| `W-SCHED-IDLE` | An exclusive lane's declared intervals leave a gap above the pacing threshold (24 ticks by default, per-lane `idleThreshold:` override). |
| `W-SCHED-ROUTESPACE-CAP` | The enum-domain cross-product for route-space checks exceeds **4096** assignments; the whole sweep (`VARIANT-GAP`/`-AMBIG`/`USER-OVERLAP`) is skipped rather than truncated. |
| `W-SCHED-TIME-MISMATCH` | A scene's first `::bg time=` disagrees with its placement's start bucket. |
| `W-SCHED-WORLD-IN-FLASHBACK` | A world placement drains inside a rewound segment. |

## See also

- The design spec (rationale, review findings, worked examples):
  [`docs/superpowers/specs/2026-08-14-lute-schedule-and-play-design.md`](superpowers/specs/2026-08-14-lute-schedule-and-play-design.md)
- The website [CLI reference](https://lute-lang.vercel.app/tooling/cli/) for every other subcommand, and `lute play --help` for the flag surface straight from the binary
- [CHANGELOG.md](../CHANGELOG.md) `0.11.0` for the release-level summary
</content>
