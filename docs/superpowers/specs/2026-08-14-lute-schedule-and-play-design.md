# Lute schedule layer + `lute play` — tick-scheduled routes and reviewable playthroughs

**Status:** design v2 — v1 reviewed (17 findings: 6 blocker, 9 major, 2 minor), all
incorporated below. Implementation proceeds against THIS version.
**Driver:** OSHiZ onboarding (eevee `packages/content-catalog/scratchpads/main-story/onboarding/lute/`).

## 1. Problem

The OSHiZ onboarding scenario is moving to a Detroit: Become Human-style progression:

- The game day is divided into **8 time-of-day buckets × 12 ticks**, over N days
  (onboarding: 7 → a 672-tick story clock).
- Scenes are **placed on the clock with a tick size**, on two lanes:
  - **user lane** — single-threaded; which scene fills a slot depends on route
    state (e.g. `run.inflow`). Empty ticks fast-forward.
  - **world lane** — events may overlap; the world does not wait for the player.
- The scenario ends when the clock runs out or a scene ends it (`::end`).

Toolchain gaps: (1) no scheduling surface (`after:` is causality, `<timeline>` is
a seconds-scale staging clock); (2) no cross-scene playthrough — reviewers cannot
see a route as the player would; (3) scene identity is conflated with filename
position (`s01c01ep14-iroha.lute`), which cannot express "same event, different
position/variant per route".

## 2. Shape of the solution

| Layer | Artifact | Gives |
|---|---|---|
| A. `schedule.yaml` | headerless CLI-owned project file beside `lute.project.yaml` | clock + lanes + guarded placements; static checks |
| B. `lute play` | new CLI command | deterministic route playthrough → reviewer transcript |
| C. naming convention | event-directory layout (project-side) | identity decoupled from position |

**`lute play` requires a schedule.** There is no schedule-less ordering fallback:
the consumer keeps branch files unguarded by design (file split IS the route), so
an `after:`-graph walk cannot select one route — it would play every sibling.
A project without `schedule.yaml` gets a hard error naming this spec.

**Out of language scope (deliberate):** `schedule.yaml` has NO `kind:`/
`luteVersion:` frontmatter, no project `uses:` resolution, no capability fold, no
IR/language version bump. Doc-kind/capability integration is a separate future
design.

## 3. Layer A — `schedule.yaml`

```yaml
clock:
  buckets: [dawn, morning, late_morning, afternoon, late_afternoon, evening, night, midnight]
  ticksPerBucket: 12          # bucket clock = 96 ticks/day
  days: 7                     # optional, default 1 — story clock = 672 ticks

lanes:
  user:  { exclusive: true }   # co-satisfiable tick overlap = error
  world: { exclusive: false }  # overlap allowed by design

placements:
  - event: kuhen-meeting            # event identity (matches scenes/<event>/)
    lane: user
    at: d1.morning+2                # `[dN.]bucket+tick` (1-based day) or absolute tick int
    size: 4                         # ticks, interval is half-open [at, at+size)
    variants:                       # per route: exactly one satisfiable (unless optional)
      - when: "run.inflow == 'iroha'"
        doc: scenes/kuhen-meeting/iroha.lute
      - when: "run.inflow == 'reiha'"
        doc: scenes/kuhen-meeting/reiha.lute
        at: d1.afternoon+0          # variants MAY override at/size/presentation —
        size: 6                     # same event, different position on this route

  - event: confinement              # cold-open flashback: story tick d7, presented first
    lane: user
    at: d7.night+4
    size: 6
    presentation: 0                 # default 100; play order = (presentation, resolved at, decl order)
    variants:
      - when: "run.inflow == 'iroha'"
        doc: scenes/confinement/iroha.lute

  - event: side-errand
    lane: user
    optional: true                  # legal to be absent on some routes (no VARIANT-GAP)
    size: 3                        # `at:` omitted → declaration-order cursor (§3.2)
    variants:
      - when: "run.inflow == 'iroha'"
        doc: scenes/side-errand/iroha.lute

  - event: nera-recon               # world lane, single-doc form (one unguarded variant)
    lane: world
    at: d6.evening+0
    size: 8
    doc: scenes/nera-recon/main.lute
```

### 3.1 Guards

`when:` reads the content-line CEL surface (state comparisons + `holds`/`count`;
NOT the `after` prerequisite profile). Route-space static checks enumerate the
cross-product of enum-typed scalar domains referenced by guards (cap 4096
assignments, above the cap `W-SCHED-ROUTESPACE-CAP` and skip); guards referencing
non-enum scalars evaluate unknown and degrade that check to a warning.

### 3.2 Cursor resolution (two-phase, static)

Phase 1 — **declaration order**: each user-lane placement's base `at` resolves in
declaration order; omitted `at` = previous user-lane placement's `at + size`
(declaration predecessor, route-independent). Omitted `at` is REJECTED
(`E-SCHED-CURSOR-DYNAMIC`) when the declaration predecessor's effective interval
is route-dependent (variant `at`/`size` overrides) — a dynamic cursor cannot be
statically checked. Phase 2 — **presentation sort**: execution order is
`(presentation, resolved at, declaration index)`. Overflow/overlap checks run on
the phase-1 resolved intervals, per variant-override combination.

### 3.3 Clock validation

Positive `ticksPerBucket`/`days`; non-empty, duplicate-free buckets; `size ≥ 1`;
`0 ≤ tick < ticksPerBucket`; days are 1-based (`d0` rejected); all arithmetic
checked (`days × buckets × ticksPerBucket`, `at + size`); intervals half-open —
adjacent placements (`end == next.at`) do not overlap.

## 4. Layer B — `lute play`

```
lute play <PROJECT_DIR>
  --state run.inflow=iroha ...      # seed scalars (route selection)
  --fact  "..." ...                 # seed facts
  --script routes/iroha-a.play.yaml # route script (choices; may also carry seeds)
  --choose kuhen-meeting/ask=ask-record ...   # ad-hoc override, event-qualified
  --auto first                      # unattended policy for unscripted decisions
  --lanes user|all                  # default: user (strict player view)
  --steps N                         # stop after N presented placements
  --json
```

### 4.1 Execution-order contract

Variant selection + tick resolution produce a **presentation-ordered execution
sequence**. Story ticks are retained for labels and world synchronization only;
tick order is a tie-break within equal presentation keys, never the primary key.
State accumulates in presentation order — the order the player experiences.

### 4.2 Chained evaluation

- **Declaration union first.** The whole gated project compiles in memory once
  (same walk/gate as `compile --all`); relations, rules, seed facts, state
  tables, prereq edges, and quest/`<on>` declarations are unioned across ALL
  documents (execution-model.md union requirement) — including quest docs, which
  are never "placed". The schedule selects which SCENE command bodies execute.
- **State tiers.** `run.*` / `user.*` / `app.*` persist across scenes; `scene.*`
  resets at every scene boundary; quest state follows the quest lifecycle. The
  runner's whole-map state is NOT carried wholesale.
- **Boundary loop.** Before each placement (presentation order): re-evaluate its
  variants' guards against current state. Exactly one satisfiable → play it;
  zero on a non-optional event → runtime `E-SCHED-VARIANT-GAP`, halt exit 1;
  two+ → `E-SCHED-VARIANT-AMBIG`, halt exit 1; zero on `optional: true` → skip.
- **Causality check.** Before a scene runs, its `after:` expression is evaluated
  against the visited/completed sets accumulated **in presentation order**; a
  violation is runtime `E-SCHED-AFTER-ORDER`. (The cold-open-first graph passes:
  day-1 scenes declare `after: visited(confinement…)` and confinement has
  already presented.)
- **`::end`** terminates the whole playthrough (complete walk, reason surfaced).

### 4.3 World lane and rewind

- **Scenes are atomic schedule units.** No intra-scene tick mapping exists.
- **Drain rule.** After a user placement completes at story end-tick `T`, world
  placements whose start tick lies within the current **segment**'s covered
  story range and which have not yet fired execute once each, atomically, in
  `(at, declaration index)` order — BEFORE the next user placement's guards are
  evaluated. `--lanes user` still EXECUTES them (state must not depend on
  rendering); it only omits them from the transcript.
- **Segments and rewind.** Consecutive user placements with non-decreasing story
  ticks form a segment; a presentation jump backward starts a new segment and is
  **purely cinematic**: no state rolls back, nothing replays, the world cursor
  restarts at the new segment's start tick. Each world placement fires at most
  once per playthrough. A world placement covered only by a segment that plays
  "in the future" of a later-presented segment gets `W-SCHED-WORLD-IN-FLASHBACK`
  (design smell: its effects would precede, in experienced order, story time it
  belongs after).

### 4.4 Route scripts (`*.play.yaml`)

Parsed by `play` itself (NOT the trace mock parser — its top-level key set is
closed; claiming compatibility was a v1 error). Sections:

```yaml
state: { run.inflow: iroha }
facts: ["..."]
choose:
  kuhen-meeting/ask: [ask-record]     # event-qualified hub/choice id
  ask: [ask-record]                   # bare id legal only if unique across the schedule
```

Bare-key ambiguity is a script error naming the colliding events. Selections per
key are consumed in visit order. An unscripted decision: `--auto first` picks the
first eligible option; otherwise the playthrough halts incomplete (exit 3) naming
the event, hub, and eligible options.

### 4.5 Honesty about the reference runtime

`play` is a **reference-runtime preview**, not a player-session emulation. Any
surface the reference runner cannot resolve (`now()`/`validAt`, `bridgeResult`
effects, bridge `wait` timing, wall-clock timeline pacing) that a guard or
transcript-visible effect actually depends on halts the walk incomplete (exit 3)
with the unresolved surface named. No silent unknowns in a review transcript.

### 4.6 Transcript

Human format: placement headers (`── d1.morning+2 (tick 26) · user ·
kuhen-meeting/iroha ──`), dialogue lines (speaker + emotion/action + text),
staging directives, choices (all offered + the one taken), fast-forward markers,
rewind markers (`⏪ d7.night+10 → d1.dawn+0 (rewind)`), end reason. `--json`:
structured stream (per-scene records, tick spans, choices offered/taken, state
deltas). Deterministic: same seeds + script ⇒ byte-identical output.

### 4.7 Coverage

`lute play --coverage routes/*.play.yaml`: replays each script, reports
placements/variants/hub-arms never visited across the corpus — the review-gap
detector.

## 5. Static checks (in `schedule.rs`, surfaced by `play` and future `check-project` integration)

| Code | Meaning |
|---|---|
| `E-SCHED-CLOCK-OVERFLOW` | resolved interval exceeds the story clock |
| `E-SCHED-USER-OVERLAP` | co-satisfiable user-lane placements with overlapping `[at, at+size)` |
| `E-SCHED-VARIANT-GAP` | non-optional event with no satisfiable variant for some route assignment |
| `E-SCHED-VARIANT-AMBIG` | two variants co-satisfiable |
| `E-SCHED-CURSOR-DYNAMIC` | omitted `at` after a route-dependent predecessor (§3.2) |
| `E-SCHED-DOC-MISSING` | placement references a nonexistent doc |
| `E-SCHED-AFTER-ORDER` | (runtime) scene's `after:` unsatisfied in presentation order |
| `W-SCHED-DOC-UNPLACED` | scene doc never referenced by any placement |
| `W-SCHED-TIME-MISMATCH` | scene's FIRST `::bg time=` disagrees with the placement's start bucket (`day` matches morning…late_afternoon); later bg changes inside a scene are not checked — no command-to-tick mapping exists |
| `W-SCHED-WORLD-IN-FLASHBACK` | world placement drained inside a rewound segment (§4.3) |
| `W-SCHED-ROUTESPACE-CAP` | route-space enumeration truncated (>4096 assignments) |
| `W-SCHED-IDLE` | user-lane gap above threshold (pacing smell) |

`E-SCHED-VARIANT-GAP` is the mechanical form of "일어날 사건은 다 일어나야 한다";
`optional: true` is the explicit opt-out.

## 6. Layer C — naming convention (project-side)

```
scenes/<event>/<variant>.lute      # scenes/confinement/iroha.lute
routes/<route>[-<label>].play.yaml
schedule.yaml
```

- **Canonical node identity is unchanged language-side**: it remains
  `<character>.s<season>ep<episode>`. The project keeps `season:`/`episode:` as
  **opaque, frozen identity numbers** (existing scenes keep their current
  numbers; new scenes take the next free number, forever). What retires is the
  rule that the number encodes reading order — position now lives ONLY in
  `schedule.yaml`, so inserting a scene is a schedule edit and never renumbers
  anything. `character: <event>-<variant>` keeps sibling ids distinct.
- Own-route vs other-route variation is a variant of the same event (`when:` +
  optional coordinate overrides) — not a differently-numbered file.

## 7. Delivery plan

1. `schedule.rs` (parse/resolve/validate, §3+§5) + `play.rs` (§4) in
   `crates/lute-cli`, feature branch `feat/schedule-play`.
2. Onboarding migration: event directories, `schedule.yaml`, route scripts,
   CONVENTIONS rewrite; per-route `lute play` transcripts into the review
   battery.
3. Future (separate design): schedule as a real doc kind + capability fold +
   `check-project` surfacing; engine-side adoption of the runtime rules in §4.3.
