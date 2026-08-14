# Lute schedule layer + `lute play` — tick-scheduled routes and reviewable playthroughs

**Status:** design, decisions resolved (user delegated: implement straight through with reviewer loops).
**Driver:** OSHiZ onboarding (eevee `packages/content-catalog/scratchpads/main-story/onboarding/lute/`).

## 1. Problem

The OSHiZ onboarding scenario is moving to a Detroit: Become Human-style progression:

- The game day is divided into **8 time-of-day buckets × 12 ticks = a 96-tick clock**
  (buckets mirror the OSHiZ wire enum `TIME_OF_DAY`).
- Scenes are **placed on the clock with a tick size**, on two lanes:
  - **user lane** — single-threaded; the player is one person. Which scene fills a
    slot depends on route state (e.g. `run.inflow`). Between placements, empty
    ticks fast-forward to the next scene.
  - **world lane** — many events may overlap; the world does not wait for the
    player. When a user-lane scene ends, the world clock syncs to the tick where
    the user scene ended.
- The scenario ends when the clock runs out, or a scene ends it explicitly (`::end`).

Three authoring/tooling gaps block this today:

1. **No scheduling surface.** Lute's `after:` graph declares causal prerequisites,
   not clock placement. The in-scene `<timeline>` is a seconds-scale local clock
   for staging clips — unrelated. Nothing says "this scene occupies ticks 52–57 on
   the user lane for the iroha route".
2. **No cross-scene playthrough.** `lute run` executes ONE compiled artifact against
   a mock; `lute trace` previews ONE source doc. Nobody — human or AI reviewer —
   can ask the toolchain: *play route X from the top and show me exactly the
   dialogue, staging, choices offered, and choices taken that a real player would
   see across the whole scenario.* Reviewers today read files in filename order,
   which is not any route's real order.
3. **Scene identity is conflated with position.** The naming convention
   (`s01c01ep14-iroha.lute`: filename = timeline slot + branch arm) cannot express
   "the same event happens on every route, but at a different position and with a
   different variant depending on the route" (confinement night, first-meeting
   ordering). Events must all occur; their order and rendering vary.

## 2. Shape of the solution

Three additions, layered so each is independently useful:

| Layer | Artifact | What it gives |
|---|---|---|
| A. Schedule document | `schedule.yaml` (new doc kind, project member) | Clock + lanes + guarded placements as data; static schedule checks |
| B. `lute play` | new CLI command (chained evaluator over compiled artifacts) | Deterministic route playthrough → the reviewer-facing transcript |
| C. Naming convention | event-directory layout (project-side, not a toolchain change) | Scene identity decoupled from clock position |

B is the heart: it makes routes *reviewable*. A makes tick placement *checkable*.
C makes variants *expressible*. B ships an MVP without A (playing the `after:`
graph in deterministic order); A upgrades B's ordering and adds the world lane.

## 3. Layer A — the schedule document

A new schema-family document (same family as `world.schema.yaml` /
`vocabulary.schema.yaml`: YAML, resolved as part of the project, folded into the
capability snapshot so `lute context` exposes it).

```yaml
kind: schedule
luteVersion: "0.11.0"

clock:
  buckets: [dawn, morning, late_morning, afternoon, late_afternoon, evening, night, midnight]
  ticksPerBucket: 12          # bucket clock = 96 ticks/day
  days: 7                     # optional, default 1 — total clock = 672 ticks

lanes:
  user:  { exclusive: true }   # overlap of co-satisfiable placements = error
  world: { exclusive: false }  # overlap allowed by design

placements:
  # --- user lane: guarded variants of one event -------------------------
  - event: kuhen-meeting            # event identity (matches scenes/<event>/)
    lane: user
    at: morning+2                   # bucket+tick (or absolute tick int)
    # `at:` addressing: `[dN.]bucket+tick` (e.g. `d7.night+4`) or absolute int.
    size: 4                         # ticks occupied
    variants:                       # exactly one must be satisfiable per route
      - when: "run.inflow == 'iroha'"
        doc: scenes/kuhen-meeting/iroha.lute
      - when: "run.inflow == 'reiha'"
        doc: scenes/kuhen-meeting/reiha.lute
  - event: errand-complication
    lane: user
    # `at:` omitted → user-lane cursor: starts where the previous user
    # placement (on this route) ended; empty ticks in between fast-forward.
    size: 3
    variants:
      - when: "run.inflow == 'iroha'"
        doc: scenes/errand-complication/iroha.lute

  # --- presentation override: cold-open flashback ------------------------
  # Story clock and PRESENTATION order may diverge (the onboarding cold-open
  # plays the day-7 confinement night first, then rewinds to day 1). Optional
  # `presentation:` overrides play order for user-lane placements: they play
  # sorted by (presentation, tick), default presentation = 100. The transcript
  # annotates the clock jump (`⏪ tick 652 → 4 (rewind)`); world-lane sync uses
  # the STORY tick, so a rewind resets the world cursor accordingly.
  - event: confinement
    lane: user
    at: d7.night+4
    size: 6
    presentation: 0
    variants:
      - when: "run.inflow == 'iroha'"
        doc: scenes/confinement/iroha.lute

  # --- world lane: overlapping background events ------------------------
  - event: nera-recon
    lane: world
    at: evening+0
    size: 8
    doc: scenes/nera-recon/main.lute      # unguarded single-doc form
```

Semantics:

- **`when:` guards are the route selector.** They read the same CEL surface as
  content-line `when=` (state comparisons allowed — NOT the `after` prerequisite
  profile). For enum-typed scalars the checker proves exhaustiveness and mutual
  exclusion, reusing the `<match>` machinery.
- **`after:` stays.** Causality (`after: "visited('...')"`) and scheduling
  (ticks) are different layers; the checker cross-validates them (§5).
- **User-lane cursor** mirrors the in-scene `<track>` cursor rule: omitted `at`
  = previous placement's end; explicit `at` places and resets the cursor.
- **World-lane sync** (user scene ends → world clock jumps to that tick) is an
  execution rule; it lives in `lute play` (§4) and the engine contract docs.

## 4. Layer B — `lute play`

```
lute play <PROJECT_DIR>
  --state run.inflow=iroha ...      # seed scalars (route selection)
  --fact  "..." ...                 # seed facts (same surface as run/trace mocks)
  --script routes/iroha-a.play.yaml # choice script: hub/choice selections in order
  --auto first                      # or: unattended policy for unscripted hubs
  --lanes user|all                  # transcript scope (default: all, world annotated)
  --until <tick>                    # partial playback
  --json                            # machine transcript
```

- **Chained evaluation.** `play` compiles every reachable placement's artifact
  and executes them **in resolved tick order with persistent state**: scalars,
  facts + Datalog fixpoint, quest lifecycle, and `visited()` all carry across
  scenes. This is `lute run`'s evaluator (the reference runtime consumer) lifted
  from one artifact to a scheduled sequence — NOT a second evaluator.
- **Route resolution.** Placements are filtered by their `when:` guards against
  current state — re-evaluated at each placement boundary, so a choice made in
  scene N legally reroutes scene N+1.
- **The transcript is the product.** Human format renders exactly the
  player-visible stream, annotated with clock context:

  ```
  ── tick 26 (morning+2) · user · kuhen-meeting/iroha ─────────────
  ::bg{location="corner_cafe" time="morning"}
  @iroha{emotion="delighted" action="hop"}: 아, 어서오세요!
  ▷ choice ask: [ask-record] ask-nothing        ← chosen: ask-record
  ⏩ tick 30 → 36 (fast-forward, empty user lane)
  ── tick 36 (afternoon+0) · world · nera-recon ───────────────────
  ── end: clock exhausted (tick 96) ───────────────────────────────
  ```

  `--json` carries the same stream structured (doc, addr, lineId, tick span,
  choices offered/taken, state deltas) for programmatic reviewers.
- **Determinism contract.** Same seeds + same script ⇒ byte-identical transcript.
  A review lane's input is `lute play` output and nothing else — the reviewer
  sees what the player sees, in the player's order, with unreached arms absent.
- **Coverage.** `lute play --coverage` over a route corpus
  (`routes/*.play.yaml`) reports unvisited placements/arms — the review-gap
  detector (extends `lute test --coverage` to the project level).
- **Failure modes.** No satisfiable variant halts with the offending event +
  state snapshot; an `unknown` guard halts as incomplete (exit 3), mirroring
  `trace` §4.4/4.5 semantics.

`*.play.yaml` (route script) reuses the mock-playthrough YAML family plus an
ordered `choices:` sequence, so a ratified route script doubles as a checked-in
regression test input.

## 5. Static checks

| Code | Meaning |
|---|---|
| `E-SCHED-CLOCK-OVERFLOW` | placement extends past the clock (at+size > total) |
| `E-SCHED-USER-OVERLAP` | two user-lane placements co-satisfiable and tick-overlapping |
| `E-SCHED-VARIANT-GAP` | some guard-domain value leaves an event with no satisfiable variant (violates "every event happens on every route") |
| `E-SCHED-VARIANT-AMBIG` | two variants of one event co-satisfiable |
| `E-SCHED-AFTER-ORDER` | tick order contradicts the `after:` graph |
| `E-SCHED-DOC-MISSING` | placement references a doc that does not exist |
| `W-SCHED-DOC-UNPLACED` | a scene doc exists but no placement references it |
| `W-SCHED-TIME-MISMATCH` | a scene's `::bg time=` disagrees with its placement's bucket |
| `W-SCHED-IDLE` | user-lane gap above a threshold (pacing smell) |

`E-SCHED-VARIANT-GAP` is the mechanical form of "일어날 사건은 다 일어나야 한다".

## 6. Layer C — naming convention (project-side)

Filename stops encoding position; the schedule is the position SSOT.

```
scenes/<event>/<variant>.lute      # scenes/confinement/iroha.lute
routes/<route>[-<label>].play.yaml # ratified route scripts
schedule.yaml
```

- Node id uniqueness: `character: <event>-<variant>` (existing namespacing
  generalized).
- The episode-number freeze rule retires: renumbering pain was a symptom of
  position-in-filename. Insertion = a schedule edit; filenames never move.
- Own-route vs other-route variation is a variant of the same event, selected
  by `when:` — not a differently-numbered file.

## 7. Resolved decisions (rationale on record)

1. **World-lane visibility:** transcript includes world-lane scenes by default,
   annotated `· world ·`. Reviewers must see the whole simultaneity design;
   whether the ENGINE surfaces a given world scene to the player is game policy,
   out of toolchain scope. `--lanes user` renders the strict player view.
2. **Bucket names:** the 8-value wire `TIME_OF_DAY` set, lowercase. `::bg time=`
   keeps its own 9-value wire vocabulary (`day` exists on the bg wire but is not
   a schedule bucket); `W-SCHED-TIME-MISMATCH` compares via bucket⊂bg mapping.
3. **Scene `size`:** authored per placement. Duration is a design lever, not a
   derivable property of line count.
4. **Schedule format:** YAML schema-family. Diffable, machine-writable by the
   storyboard workbench, consistent with world/vocabulary.
5. **Inflow seeding:** `--state` seeds are sufficient; no pre-roll hook. The
   onboarding contract already assumes upstream assignment before scene 1.

## 8. Delivery plan

1. **`lute play` MVP:** chained evaluator + transcript + route scripts; ordering
   from the schedule when present, else deterministic `after:`-graph topological
   order. Unblocks AI route review immediately.
2. **Schedule layer:** doc kind, capability fold, §5 checks.
3. **Onboarding migration:** event-directory rename, `schedule.yaml`, CONVENTIONS
   rewrite, route script corpus for the 6 inflow routes, per-route transcripts
   reviewed by the review battery.
