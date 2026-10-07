# Timeline

**Semantic id:** `lute.timeline/1`.

## Constructs and rules

- **Timeline/track/clip:** `<timeline>` contains tracks and timed directives/`::set`; each track has a local placement cursor and omitted `at` follows its previous end.
- **Ordering:** clips flatten to records whose `timing` object carries `at` (scheduled placement in seconds) and `timeline` (the zero-based ordinal of the scheduled timeline in the document, not a second count), then sort by absolute placement and track index. Same-track overlaps and conflicting cross-track writes are diagnostics.
- **Barrier:** a `barrier` record (`family: "control"`) joins all clips before the walk continues. It has no `timing` object; its required `timeline` ordinal and `at` seconds are direct fields. Per-command `wait`, `duration`, and `delay` live in the record's `timing` object and are owned by the command's semantic id (`lute.staging/1` for staging directives, otherwise `lute.core/1`), not by `lute.timeline/1`. `duration`, `delay` and `at` must be finite non-negative seconds.

## Evaluation and lowering

Timeline validation and flattening preserve clip placement and barrier joins before ordinary command order. The current reference executor records a barrier without a real timeline clock; per-command timing remains with the owning command. See [module ordering](../design/modules.md#timeline).

## Diagnostics

Placement (`timing.at`, `timing.timeline`), duplicate tracks, overlaps, cross-track write conflicts and timeline-only admissions are checked. Command-level blocking and durations are checked by the owning command module.

## Example

```lute check
---
kind: scene
id: tl
---
## Intro {#intro}
<timeline duration="2.0">
  <track channel="music">
    ::music{assetId="theme", at="0.0", duration="2.0"}
  </track>
</timeline>
```

lowers to:

```json
[
  {"kind": "music", "family": "staging", "position": "001-0100", "assetId": "theme", "timing": {"duration": 2.0, "at": 0.0, "timeline": 0}},
  {"kind": "barrier", "family": "control", "position": "001-0200", "timeline": 0, "at": 2.0}
]
```

## History

[DSL 0.37 §5.2](../proposals/scenario-dsl/0.37.0.md#52-timing-and-trace), [DSL 0.33 §1–2](../proposals/scenario-dsl/0.33.0.md#1-domain-modules-and-evaluation-order), [timeline command handling](../../crates/lute-trace/src/exec/machine/commands.rs).
