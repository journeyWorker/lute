# Timeline

**Semantic id:** `lute.timeline/1`.

## Constructs and rules

- **Timeline/track/clip:** `<timeline>` contains tracks and timed directives/`::set`; each track has a local placement cursor and omitted `at` follows its previous end.
- **Ordering:** clips flatten to records carrying `Stamp.timeline` and `Stamp.at`, then sort by absolute placement and track index. Same-track overlaps and conflicting cross-track writes are diagnostics.
- **Barrier:** a `barrier` joins all clips before the walk continues. Per-command `wait`, `duration`, and `delay` are owned by the command's semantic id (`lute.staging/1` for staging directives, otherwise `lute.core/1`), not by `lute.timeline/1`.

## Evaluation and lowering

Timeline validation and flattening preserve clip placement and barrier joins before ordinary command order. The current reference executor records a barrier without a real timeline clock; per-command timing remains with the owning command. See [module ordering](../design/modules.md#timeline).

## Diagnostics

Placement/`Stamp.timeline`/`Stamp.at`, duplicate tracks, overlaps, cross-track write conflicts and timeline-only admissions are checked. Command-level blocking and durations are checked by the owning command module.

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
