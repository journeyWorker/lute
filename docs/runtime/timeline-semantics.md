# Timeline semantics

A `<timeline>` stages parallel `<track>`s of `<clip>`s onto a shared **local
clock** (`crates/lute-check/src/timeline.rs`). The compiler flattens each
timeline during stage resolution (`crates/lute-compile/src/schedule.rs`) and
emits its clips as ordinary command records **plus timing stamps**, closed by a
`barrier` record. The engine replays that pre-scheduled stream on its own clock;
the compiler has already done the scheduling math.

## What the IR carries

Records emitted inside a timeline carry the cross-cutting `Stamp` fields
(`ir.rs::Stamp`, flattened onto the record):

- `timeline` — the 0-based **timeline ordinal** (`u32`) this record belongs to;
- `at` — the record's **absolute start time** on the timeline's local clock
  (seconds);
- `duration` — the record's resolved duration, when known;
- `delay` — a relative nudge, when authored.

The timeline is closed by a **`barrier` command** (`ir.rs::BarrierCmd`, no
stamp): `{ kind: "barrier", addr, timeline, at }`, where `at` is the barrier
time. Clips are emitted in deterministic **`(at, track index)` order**
(`schedule.rs` sorts by `at`, then track index, stable on ties so same-`(at,
track)` clips keep document order).

## The local clock and per-track cursors

Each `<track>` carries an **independent cursor** (§11.4 sequential-omission,
`timeline.rs`):

- a clip with an **omitted `at`** starts at `0.0` when it is the track's first
  clip, otherwise immediately after the previous clip's **end**
  (`prev.at + prev.duration`);
- an **explicit `at`** places the clip there and resets the cursor to that
  clip's end;
- a clip's duration comes from its directive's `duration` timing attr (§7.5),
  best-effort parsed.

The clock is **local to the timeline** — `at` values are offsets within this
timeline, not global positions.

## The barrier (join)

`barrier_at` is the timeline's explicit `<timeline duration>` when present
(parsed best-effort as `f64`), otherwise the **maximum clip end across all
tracks** (`0.0` for an empty timeline). The engine treats the `barrier` record
as a **join point**: it must not advance past the barrier until every clip
scheduled before `barrier_at` on every track of that `timeline` ordinal has
played. The node *after* the timeline in the command stream sees the timeline's
resulting state, never stale pre-timeline state (`stage.rs`).

## Write invariants the compiler guarantees

The engine may run tracks concurrently because the checker has statically ruled
out the races that would make that unsafe. All are `Layer::Staging` diagnostics
raised at compile time (`timeline.rs`):

- **`E-DUP-TRACK`** — two `<track>`s share the same track key. So a `timeline`
  ordinal's tracks are distinct.
- **`E-CLIP-OVERLAP`** — two clips in the **same** track whose
  `[at, at+duration)` half-open intervals overlap. So within one track, at most
  one clip is active at any instant — a track is a single sequential writer.
- **`E-WRITE-CONFLICT`** — two clips on **different** tracks whose resolved
  state-write targets overlap (equal, or one a dotted-boundary prefix of the
  other) at overlapping times. So **no two parallel tracks write the same state
  target concurrently**: every state target has at most one writer at any
  instant across the whole timeline.
- **`E-CLIP-TIMING`** — one clip carrying both `at` and `delay` (mutually
  exclusive).
- **`E-TIMELINE-DURATION`** — an explicit `duration` below the max resolved
  clip end (a timeline may not truncate its own content).

Advisory size warnings also exist (`W-TIMELINE-TRACKS` >8 tracks,
`W-TIMELINE-CLIPS` >12 clips in a track, `W-TIMELINE-TOTAL` >40 total).

The `E-WRITE-CONFLICT` model resolves each clip's write targets from its
directive's `effects.writes[]` (a `::set` writes its path; a known effectless
directive writes nothing; an unknown directive or an unresolvable `fromAttr`
falls back to the coarse track subject as a single conservative target). This is
a **conservative** analysis — an unresolvable target widens to the whole track
subject rather than risk a missed conflict.

## Engine contract

Given the above, an engine has two sound options for a timeline, and the
write-conflict guarantee makes both equivalent in observable state:

1. **Replay the pre-scheduled order.** Play the emitted records in their
   `(at, track)` order, honoring `at`/`duration`/`delay` against its clock,
   then apply the `barrier` join.
2. **Run tracks concurrently.** Drive each track's clips on the local clock in
   parallel; because no two tracks write the same target at overlapping times,
   there is no write race, and the `barrier` synchronizes them before
   continuing.

The DSL fixes the *scheduling* (cursor math, barrier time) and the *write
invariants*; it does **not** mandate a threading model. Anything beyond
"honor `at`/`duration`, respect the barrier, trust the no-conflict guarantee"
— frame pacing, interpolation between keyframes, audio mixing — is engine
policy and is left unspecified here.
