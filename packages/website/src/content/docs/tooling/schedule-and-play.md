---
title: Schedule & play
description: "`schedule.yaml` — the tick clock, `user`/`world` lanes, and guarded placements that place a project's scenes in reading order — and `lute play`, the whole-project chained playthrough command with `--coverage` review-gap reporting."
---

`schedule.yaml` places a project's scenes on a tick clock instead of leaving
reading order to file position; `lute play <PROJECT_DIR>` walks that schedule
into one chained, reviewer-facing transcript — the order a player following
one route actually sees. Both are **toolchain-only**: `schedule.yaml` carries
no `kind:`, no `luteVersion:`, no capability fold, and neither the language
nor the IR moved to add them — nothing in `lute check`/`lute compile` has
ever heard of it. `lute play` refuses a project with no `schedule.yaml`
outright (exit **2**): sibling route files are deliberately unguarded (file
split *is* the route), so an `after:`-graph walk can't select one route
through them — it would play every sibling. Full key and diagnostic
reference (this page condenses it): [`docs/schedule-and-play.md`](https://github.com/journeyWorker/lute/blob/main/docs/schedule-and-play.md).

## `schedule.yaml`

```yaml
clock:
  buckets: [dawn, morning, late_morning, afternoon, late_afternoon, evening, night, midnight]
  ticksPerBucket: 12
  days: 7

lanes:
  user:  { exclusive: true, idleThreshold: 0 }   # single-threaded, guarded against overlap
  world: { exclusive: false }                    # overlap by design

assume:
  - "run.inflow != 'none'"                       # narrows the route-space sweep, never hides a gap

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
        at: d2.afternoon+0       # same event, a different position on this route
        size: 6
```

`clock:` is `buckets` (named, ordered, no duplicates) × `ticksPerBucket` ×
`days` (1-based; `d0` is rejected) — the story clock is their product,
checked for `u32` overflow. `lanes:` names any set of lanes; `exclusive:
true` guards co-satisfiable placements against overlapping intervals
(`E-SCHED-USER-OVERLAP`), `exclusive: false` allows it by design. `assume:`
is a list of guard-surface CEL strings that prunes route-space assignments
the schedule can prove will never occur (an upstream contract like "inflow is
never `none`") from the static gap/ambiguity/overlap sweep — it narrows,
never hides a real gap.

Each `placements:` entry is one **event** on a `[at, at+size)` interval on
one **lane**, in one of two forms: a single unguarded `doc:` directly on the
placement, or a `variants:` list (`when`/`doc`/`at?`/`size?`/`presentation?`)
where at most one is satisfiable per route. Giving neither form, both, or an
empty `variants:` is `E-SCHED-VARIANT-FORM`. `optional: true` legalizes zero
satisfiable variants for some route (no `E-SCHED-VARIANT-GAP`) — for content
only some lanes have written yet. `presentation` (default `100`, lower plays
first) decouples *when a scene is presented* from *when it happens on the
clock* — a variant may override its placement's `at`/`size`/`presentation`,
which is how "same event, different position on this route" is expressed,
never a differently-numbered file.

### `at:` grammar

`[dN.]<bucket>+<tick>` (`dN.` optional, defaults to day 1; `0 ≤ tick <
ticksPerBucket`) or a bare non-negative absolute tick integer. Malformed
shape, `d0`, an unknown bucket, or an out-of-range tick is
`E-SCHED-AT-PARSE`; a well-formed coordinate that resolves past the clock (or
overflows resolving it) is the *different* code `E-SCHED-CLOCK-OVERFLOW`, so
an overflow is never misread as a typo.

An **omitted** `at:` inherits the previous placement on the *same lane*'s
resolved `at + size` (declaration order) — unless that predecessor's own
`at`/`size` is route-dependent (a variant override), in which case a static
cursor can't be computed and it's `E-SCHED-CURSOR-DYNAMIC`. Execution order
is a separate, later sort: `(presentation, resolved at, declaration index)`.

## `lute play`

```console
$ lute play <PROJECT_DIR>
    --state run.inflow=iroha ...              # seed scalars (route selection)
    --fact "..." ...                          # seed facts
    --script routes/iroha.play.yaml           # route script (its own closed grammar)
    --choose kuhen/coffeeOrder=recommend ...  # ad-hoc override, event-qualified
    --auto first                              # unattended policy for unscripted decisions
    --lanes user|all                          # default user (strict player view)
    --steps N                                 # stop after N presented placements
    --coverage <FILE>...                      # review-gap corpus replay (repeatable)
    --json
```

Compiles the WHOLE gated project once in memory (scene *and* quest kind, the
same declaration union `compile --all` writes), then walks the schedule's
user-lane placements in **presentation** order — `(presentation, resolved
at, declaration index)`, never file order or story tick — re-evaluating each
event's guarded variants against live state and threading `run.*`/`user.*`/
`app.*`/`quest.*` state and facts across scene boundaries through `lute
run`'s own reference evaluator (`scene.*` always resets at every boundary).
A cold-open flashback declared `presentation: 0` can legitimately play first
even though its story tick is chronologically last.

Route selection is `--state`/`--fact` seeds, a `--script <route>.play.yaml`
(this command's **own** closed grammar — `state:`/`facts:`/`choose:` with
event-qualified ids like `kuhen/coffeeOrder: [recommend]`, never the `lute
trace --mock` parser, whose grammar has no notion of that shape), and/or
ad-hoc `--choose <event>/<id>=<choiceId>[,<choiceId>…]`; a bare
(unqualified) id is legal only when unique across the whole schedule.
`--auto first` resolves anything left unscripted, at every hub
re-presentation, not just the first. CLI flags win over a route script's
`state:`/`choose:` on a same-key conflict; facts union.

Before each placement: its variants' guards re-evaluate against current
state — exactly one satisfiable plays it, zero on a non-`optional` event is
`E-SCHED-VARIANT-GAP`, two or more is `E-SCHED-VARIANT-AMBIG` (both halt exit
**1**). A scene's `after:` prerequisite is checked against the
visited/completed sets accumulated in **presentation** order —
`E-SCHED-AFTER-ORDER` on a violation, exit **1**. Quest state is data-unioned
project-wide (a guard referencing `quest.<id>.state` type-checks) but the
`<quest>`/`<on>` lifecycle is never driven across scene boundaries — a
deliberate scoping decision, so `completed(...)`/`active(...)` always
evaluate against an empty set, and a placement causally gated on quest
completion always halts `E-SCHED-AFTER-ORDER` (exit **1**), never a silent
pass.

### World lane and rewind

After a user placement completes, every not-yet-fired world placement whose
start tick falls inside the segment just covered drains atomically, in
`(at, declaration index)` order, even under `--lanes user` (world scenes
still execute — state must not depend on rendering — the flag only gates the
transcript). A presentation jump backward starts a new segment and is purely
cinematic: no state rolls back, nothing replays. A world placement draining
inside a segment that plays "in the future" of a later-presented one is
`W-SCHED-WORLD-IN-FLASHBACK` — a design smell, not a halt.

### Transcript

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
── end: clock exhausted (tick 672) ──────────────────────────
```

`──` headers name `<tickLabel> (tick <N>) · <lane> · <event>/<variant>`;
`⏩`/`⏪` mark fast-forward across an empty user lane and a cinematic rewind;
`▷ choice <id>: opt1 [chosen] opt2 ← chosen: opt2` restates every offered
option per decision, one line per hub re-presentation as `once` arms retire;
an unscripted decision prints `← INCOMPLETE (no decision)` and the halt
names the event, doc, kind, id, and every still-eligible option. `--json`
emits `{exit, endReason, scenes: [...]}` — each scene carries
`event`/`variant`/`doc`/`lane`/`tick`/`tickLabel`/`endTick`/`stateDelta`/
`fastForwardFrom`/`rewindFrom`/`timeMismatch`/`worldInFlashback` plus
`commands` (the same records `lute run --json` emits, addressed by `addr`).
Deterministic: same seeds and script produce byte-identical output.

### Coverage (`--coverage`)

Replays every named route script through the same chain executor,
per-script transcript suppressed, and reports every placement, variant, and
hub/choice option the corpus as a whole never exercises. `--coverage <FILE>`
is repeatable and takes exactly **one file per flag** — there is no glob
expansion inside the flag itself; a shell glob passed directly
(`--coverage routes/*.play.yaml`) is a clap usage error, because the shell
already expanded it into extra bare positional arguments the command
refuses:

```console
$ lute play . --coverage routes/*.play.yaml
error: unexpected argument 'routes/ann.play.yaml' found
```

Shell-expand the glob into one `--coverage` per file instead. Exclusive with
`--script`/`--choose`/`--steps` — a single playthrough's own knobs don't
compose with a corpus replay.

## Exit codes

| Code | Plain `lute play` | `--coverage` |
|---|---|---|
| `0` | Complete — `::end` or clock exhausted. | Full coverage. |
| `1` | Halted, named by its `E-SCHED-*` code (`VARIANT-GAP`/`VARIANT-AMBIG`/`AFTER-ORDER`), or a static schedule/route-space error caught before compiling. | A coverage gap remains. |
| `2` | No `schedule.yaml`, a malformed route script, a project vocabulary conflict, `--coverage` combined with `--script`/`--choose`/`--steps`, or a usage error. | Same. |
| `3` | **Incomplete** — an unscripted decision with no `--auto` policy, or an unresolved reference-runtime surface (below). | At least one corpus script itself halted incomplete. |

**Unsupported surfaces**, each honestly surfaced rather than silently
decided: `now()`/`validAt(...)` and an unresolved plugin `bridgeResult`
effect halt **incomplete (exit 3)**, naming the event and doc; a quest-chain
causality gate (`after:` on `completed`/`active`) is the *different* tier
above — always `E-SCHED-AFTER-ORDER`, **exit 1** — since the empty
completed/active set is a defined, if pessimistic, answer. Wall-clock
`<timeline>` pacing is never simulated and never gates a halt on its own.

## Diagnostics

Fifteen static errors, one runtime error, five warnings — the full
`E-SCHED-*`/`W-SCHED-*` set.

| Code | Meaning |
|---|---|
| `E-SCHED-CLOCK-STRUCTURE` | `ticksPerBucket`/`days` is `0`, or `buckets` is empty. |
| `E-SCHED-BUCKET-DUP` | The same bucket name twice. |
| `E-SCHED-LANE-UNKNOWN` | A placement's `lane:` names no declared lane. |
| `E-SCHED-EVENT-DUP` | The same `(event, lane)` placed twice. |
| `E-SCHED-VARIANT-FORM` | Neither `doc:` nor `variants:`, both, or an empty `variants:`. |
| `E-SCHED-SIZE-INVALID` | A resolved `size` is `0`. |
| `E-SCHED-AT-PARSE` | A malformed `at:` shape. |
| `E-SCHED-CURSOR-DYNAMIC` | Omitted `at:` after a route-dependent same-lane predecessor. |
| `E-SCHED-CLOCK-OVERFLOW` | A resolved interval exceeds the clock, or overflows resolving it. |
| `E-SCHED-DOC-MISSING` | A project-relative `doc:` names no existing file. |
| `E-SCHED-DOC-PATH` | `doc:` is absolute or escapes the project root. |
| `E-SCHED-VARIANT-GAP` | A non-`optional` placement has no satisfiable variant for some route. |
| `E-SCHED-VARIANT-AMBIG` | Two variants co-satisfiable for some route. |
| `E-SCHED-USER-OVERLAP` | Overlapping intervals, co-satisfiable, on an exclusive lane. |
| `E-SCHED-GUARD-PARSE` | A `when:`/`assume:` CEL guard fails to parse. |
| `E-SCHED-AFTER-ORDER` | (runtime) `after:` unsatisfied in presentation order. |
| `W-SCHED-DOC-UNPLACED` | A scene doc exists but no placement references it. |
| `W-SCHED-IDLE` | An exclusive lane's gap exceeds the pacing threshold. |
| `W-SCHED-ROUTESPACE-CAP` | Route-space enumeration exceeds 4096 assignments; the sweep is skipped. |
| `W-SCHED-TIME-MISMATCH` | A scene's first `::bg time=` disagrees with its placement's bucket. |
| `W-SCHED-WORLD-IN-FLASHBACK` | A world placement drains inside a rewound segment. |

Full field-by-field key reference, worked examples, and the naming
convention live in [`docs/schedule-and-play.md`](https://github.com/journeyWorker/lute/blob/main/docs/schedule-and-play.md); the design rationale is the [schedule + play design spec](https://github.com/journeyWorker/lute/blob/main/docs/superpowers/specs/2026-08-14-lute-schedule-and-play-design.md).
</content>
