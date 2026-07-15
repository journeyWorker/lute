---
title: Timeline & property tracks
description: The <timeline> multi-track choreography block — a local clock, subject/channel/property tracks, absolute at offsets, and the one-writer-per-track rule.
---

A `<timeline>` is a bounded, non-interactive **choreography unit** with its own local clock. Where
consecutive non-`wait` directives give you loose concurrency, a timeline gives you *temporal
scoping*: several **tracks** each hold time-positioned staging clips, all tracks play concurrently
as the playhead advances, and the whole block blocks the following content until it completes.

```lute
<timeline duration="1.4">
  <track subject="camera">
    ::camera{focus="bianca" zoom="1.35" duration="0.4"}
    ::camera{shake="0.6" duration="0.3" at="0.5"}
  </track>
  <track channel="fg">
    ::cut{assetId="CUT.scenarios.bianca.s01ep02.01" at="0.5"}
    ::cut{assetId="CUT.scenarios.bianca.s01ep02.01" action="hide" at="1.1"}
  </track>
  <track channel="vfx">
    ::vfx{type="whiteOut" transition="flash" at="0.5"}
  </track>
  <track channel="sfx">
    ::sfx{sound="finger_beam_flash" assetId="PLACEHOLDER_finger_beam_flash" at="0.5"}
  </track>
</timeline>
```

*(From [`docs/examples/bianca-s01ep02.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/bianca-s01ep02.lute) — the "finger-beam performance" as one four-track beat.)*

## Tracks and keys

A `<track>` is identified by its key, given one of three ways:

- **`subject="camera"`** — a subject track (a whole staged entity, e.g. the camera or a character).
- **`channel="fg"`** — a channel track (a staging channel such as foreground cut-ins, vfx, sfx,
  music).
- **`subject="bianca" property="pos"`** — a **property track**: split one subject across
  property-scoped concurrent tracks.

Each track key must be **distinct** — one writer per key. Two `subject="camera"` tracks would
silently fight and are rejected; a track with no usable key is `E-TRACK-KEY`.

### Property tracks

Property tracks let one subject animate on several properties at once. Here `bianca` is driven on
her position and her opacity simultaneously, alongside an independent camera track:

```lute
<timeline duration="1.2">
  <track subject="bianca" property="pos">
    ::auto{character="bianca" anchor="left" action="slide-in-left"}
  </track>
  <track subject="bianca" property="opacity">
    ::auto{character="bianca" action="fade-in-up"}
  </track>
  <track subject="camera">
    ::camera{focus="bianca" zoom="1.2" duration="0.6"}
    ::camera{shake="0.4" duration="0.3" at="0.7"}
  </track>
</timeline>
```

*(From [`docs/examples/property-tracks.lute`](https://github.com/KantoRegion/lute/blob/main/docs/examples/property-tracks.lute).)*

The keys `bianca.pos`, `bianca.opacity`, and `camera` are all distinct, so the checker accepts them.

## The time model

Within a `<timeline>` the local clock starts at `0`. For each track:

- a clip's **`at`** is an **absolute** time on that clock — never a relative nudge;
- an **omitted** `at` starts the clip after the previous clip in that track (the first clip starts
  at `0.0`);
- clips lower to the same flat command records, sorted by resolved time, with a final barrier at
  `duration` (or the maximum resolved end).

## Constraints

Tracks are **staging-only and non-interactive**: they hold `::` staging leaves (plus `::set` for
state marks). No dialogue, prose, `<choice>`, `<branch>`, or `<match>` may appear inside — those
would make the beat reader-paced rather than clock-paced. Timelines do not nest, and are not
admitted in quest documents. The checker warns when a timeline grows unwieldy (past roughly 8
tracks, 12 clips per track, or 40 clips total).
