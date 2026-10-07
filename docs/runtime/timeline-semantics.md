# Timeline semantics

A `<timeline>` stages parallel `<track>`s of clips onto a shared **local
clock** (`crates/lute-check/src/timeline.rs`). The compiler flattens each
timeline during stage resolution (`crates/lute-compile/src/schedule.rs`) and
emits its clips as ordinary command records **with a `timing` object**, closed
by a `barrier` record. The engine replays that pre-scheduled stream on its own
clock; the compiler has already done the scheduling math.

## What the IR carries

Records emitted inside a timeline carry these members of their `timing`
object (`ir.rs::Timing`), beside whatever `wait` the directive resolved:

- `timeline` — the **timeline ordinal** (`u32`): zero-based, assigned in
  document order (`stage.rs`), so the first `<timeline>` in a document is `0`.
  It is an ordinal, not a second count; treat it as a correlation key that
  ties clips to their barrier within one artifact, never as a durable
  identity;
- `at` — the record's **absolute start time** on the timeline's local clock
  (seconds);
- `duration` — the record's resolved duration in seconds, when known;
- `delay` — a relative nudge in seconds, when authored.

The timeline is closed by a **`barrier` command** (`ir.rs::BarrierCmd`,
`family: "control"`, no `timing`): `{ kind, family, position, timeline, at }`,
where `timeline` is the same ordinal and `at` is the barrier time in seconds.
The barrier's `timeline` and `at` are direct fields, never inside a `timing`
object. Clips are emitted in deterministic **`(at, track index)` order**
(`schedule.rs` sorts by `at`, then track index, stable on ties so same-`(at,
track)` clips keep document order).

```lute check
---
kind: scene
id: dock-timeline
title: Dock timeline
enums:
  framing: [wide]
---

## Dock

<timeline>
  <track subject="camera">
    ::camera{focus="hero" framing="wide" duration="1.5"}
  </track>
  <track subject="sfx">
    ::sfx{sound="bell" at="0.5" duration="0.5"}
  </track>
</timeline>
@narrator{code="after"}: The bell fades.
```

compiles to (`lute compile`, first three records):

```json
[
  {
    "kind": "camera",
    "family": "staging",
    "position": "001-0100",
    "focus": "hero",
    "framing": "wide",
    "timing": {
      "wait": false,
      "duration": 1.5,
      "at": 0.0,
      "timeline": 0
    }
  },
  {
    "kind": "sfx",
    "family": "staging",
    "position": "001-0200",
    "sound": "bell",
    "timing": {
      "duration": 0.5,
      "at": 0.5,
      "timeline": 0
    }
  },
  {
    "kind": "barrier",
    "family": "control",
    "position": "001-0300",
    "timeline": 0,
    "at": 1.5
  }
]
```

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
   `(at, track)` order, honoring `timing.at` / `timing.duration` /
   `timing.delay` against its clock, then apply the `barrier` join.
2. **Run tracks concurrently.** Drive each track's clips on the local clock in
   parallel; because no two tracks write the same target at overlapping times,
   there is no write race, and the `barrier` synchronizes them before
   continuing.

The DSL fixes the *scheduling* (cursor math, barrier time) and the *write
invariants*; it does **not** mandate a threading model. Anything beyond
"honor `timing.at`/`timing.duration`, respect the barrier, trust the no-conflict guarantee"
— frame pacing, interpolation between keyframes, audio mixing — is engine
policy and is left unspecified here.
