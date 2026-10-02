# Timeline

**Semantic id:** `lute.timeline/1`.

## Constructs and rules

- **Timeline/track/clip:** `<timeline duration>` contains tracks and timed directives/`::set`; each track has a local cursor, omitted `at` follows its previous end, and `at` plus `delay` is invalid.
- **Ordering:** clips flatten to ordinary records with `Stamp{timeline, at, duration, delay}` and sort by absolute `at`, then track index. Same-track overlaps and conflicting cross-track writes are diagnostics.
- **Barrier:** timeline completion emits a `barrier` that joins all clips before the next command; engines must not advance past it early.

## Evaluation and lowering

Timeline validation and flattening precede execution; local clocks schedule clips, then the barrier returns to ordinary command order. The current reference executor records a barrier without a real timeline clock; see [module ordering](../design/modules.md#timeline).

## Diagnostics

Duration/at/delay types, duplicate tracks, overlaps, cross-track write conflicts and timeline-only admissions are checked. Async bridge failure/timeout policy belongs to the bridge host, not timeline syntax.

## Example

```lute
<timeline duration="2.0">
  <track channel="music">
    ::music{asset="theme", at="0.0", duration="2.0"}
  </track>
</timeline>
```

## History

[DSL 0.33 §1–2](../proposals/scenario-dsl/0.33.0.md#1-domain-modules-and-evaluation-order), [timeline command handling](../../crates/lute-trace/src/exec/machine/commands.rs).
